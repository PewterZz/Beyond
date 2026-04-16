//! WebSocket server that accepts one paired phone at a time and bridges
//! `ServerMsg` / `ClientMsg` frames over binary CBOR.
//!
//! Performance notes:
//! - Uses `tokio_tungstenite::accept_async` with default per-message deflate
//!   disabled (the phone does its own compression on images; text is small).
//! - Outbound messages are sent on a `mpsc::channel::<ServerMsg>(256)`; if the
//!   phone lags, `AgentTextDelta` frames are the only thing dropped — block
//!   creation and completion always make it through.
//! - Connection stays open indefinitely; a 15s ping keeps NATs/routers warm.

use crate::protocol::{ClientMsg, Hello, ServerMsg, PROTOCOL_VERSION};
use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};

pub type ServerTx = mpsc::Sender<ServerMsg>;
pub type ClientRx = mpsc::UnboundedReceiver<ClientMsg>;

/// Handle returned to the UI — drop it to shut the server down.
pub struct ServerHandle {
    pub port: u16,
    /// Fan-out sender. Every connected client gets its own forwarder task that
    /// subscribes to this broadcast.
    pub outbound: broadcast::Sender<ServerMsg>,
    /// Inbound messages from any connected client.
    pub inbound_rx: Arc<Mutex<mpsc::UnboundedReceiver<ClientMsg>>>,
    pub connected: Arc<std::sync::atomic::AtomicBool>,
    pub connect_gen: Arc<std::sync::atomic::AtomicU64>,
    _shutdown: mpsc::Sender<()>,
}

pub struct ServerConfig {
    pub bind: SocketAddr,
    pub token: String,
    pub session_id: String,
    pub active_model: String,
    pub active_provider: String,
}

pub async fn start(cfg: ServerConfig) -> Result<ServerHandle> {
    let listener = TcpListener::bind(cfg.bind)
        .await
        .with_context(|| format!("bind remote server on {}", cfg.bind))?;
    let port = listener.local_addr()?.port();

    let (outbound_tx, _) = broadcast::channel::<ServerMsg>(4096);
    let (inbound_tx, inbound_rx) = mpsc::unbounded_channel::<ClientMsg>();
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
    let connected = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let connect_gen = Arc::new(std::sync::atomic::AtomicU64::new(0));

    let outbound_tx_c = outbound_tx.clone();
    let connected_c = connected.clone();
    let connect_gen_c = connect_gen.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    info!("remote: shutting down");
                    break;
                }
                accept = listener.accept() => {
                    match accept {
                        Ok((stream, addr)) => {
                            if connected_c.load(std::sync::atomic::Ordering::Relaxed) {
                                warn!("remote: rejecting {addr}; another phone is paired");
                                drop(stream);
                                continue;
                            }
                            let ob = outbound_tx_c.clone();
                            let ib = inbound_tx.clone();
                            let connected = connected_c.clone();
                            let token = cfg.token.clone();
                            let hello = Hello {
                                version: PROTOCOL_VERSION,
                                server_name: "beyonder".into(),
                                session_id: cfg.session_id.clone(),
                                active_model: cfg.active_model.clone(),
                                active_provider: cfg.active_provider.clone(),
                            };
                            let gen = connect_gen_c.clone();
                            tokio::spawn(async move {
                                connected.store(true, std::sync::atomic::Ordering::Relaxed);
                                gen.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                if let Err(e) = handle_client(stream, addr, token, hello, ob, ib).await {
                                    warn!("remote: client {addr} ended: {e}");
                                }
                                connected.store(false, std::sync::atomic::Ordering::Relaxed);
                            });
                        }
                        Err(e) => warn!("remote: accept error: {e}"),
                    }
                }
            }
        }
    });

    Ok(ServerHandle {
        port,
        outbound: outbound_tx,
        inbound_rx: Arc::new(Mutex::new(inbound_rx)),
        connected,
        connect_gen,
        _shutdown: shutdown_tx,
    })
}

async fn handle_client(
    stream: tokio::net::TcpStream,
    addr: SocketAddr,
    token: String,
    hello: Hello,
    outbound: broadcast::Sender<ServerMsg>,
    inbound_tx: mpsc::UnboundedSender<ClientMsg>,
) -> Result<()> {
    // Nagle off — small keystroke-rate frames need low latency.
    let _ = stream.set_nodelay(true);
    let ws = tokio_tungstenite::accept_async(stream)
        .await
        .context("ws handshake")?;
    let (mut sink, mut src) = ws.split();
    info!("remote: {addr} connected");

    // Require auth frame first.
    let auth_deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(5);
    let authed = loop {
        let timeout = auth_deadline.saturating_duration_since(tokio::time::Instant::now());
        if timeout.is_zero() {
            break false;
        }
        match tokio::time::timeout(timeout, src.next()).await {
            Ok(Some(Ok(Message::Binary(buf)))) => match decode_client(&buf) {
                Ok(ClientMsg::Auth { token: t }) if t == token => break true,
                _ => break false,
            },
            Ok(Some(Ok(Message::Ping(p)))) => {
                let _ = sink.send(Message::Pong(p)).await;
            }
            _ => break false,
        }
    };
    if !authed {
        let _ = sink
            .send(encode_server(&ServerMsg::Error {
                message: "auth failed".into(),
            }))
            .await;
        return Ok(());
    }

    // Greet.
    sink.send(encode_server(&ServerMsg::Hello(hello)))
        .await
        .ok();

    let mut rx = outbound.subscribe();
    let mut ping_tick = tokio::time::interval(tokio::time::Duration::from_secs(15));
    ping_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(m) => {
                        if sink.send(encode_server(&m)).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        debug!("remote: phone lagged, {n} frames dropped");
                    }
                    Err(_) => break,
                }
            }
            frame = src.next() => {
                match frame {
                    Some(Ok(Message::Binary(buf))) => {
                        match decode_client(&buf) {
                            Ok(msg) => {
                                let _ = inbound_tx.send(msg);
                            }
                            Err(e) => warn!("remote: decode: {e}"),
                        }
                    }
                    Some(Ok(Message::Ping(p))) => { let _ = sink.send(Message::Pong(p)).await; }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(e)) => {
                        warn!("remote: ws error: {e}");
                        break;
                    }
                    _ => {}
                }
            }
            _ = ping_tick.tick() => {
                if sink.send(Message::Ping(vec![])).await.is_err() {
                    break;
                }
            }
        }
    }
    info!("remote: {addr} disconnected");
    Ok(())
}

/// Magic byte prefixed to zstd-compressed frames so the phone can distinguish
/// compressed from raw CBOR. Raw CBOR never starts with 0xFF (it's the CBOR
/// "break" code which is invalid at top level).
const ZSTD_MAGIC: u8 = 0xFF;

/// Compression level — 1 is fastest, good enough for sub-ms at PTY frame sizes.
const ZSTD_LEVEL: i32 = 1;

/// Minimum CBOR payload size to bother compressing. Below this the zstd framing
/// overhead can actually make the message larger.
const COMPRESS_THRESHOLD: usize = 128;

fn encode_server(msg: &ServerMsg) -> Message {
    let mut cbor = Vec::with_capacity(256);
    if ciborium::into_writer(msg, &mut cbor).is_err() {
        return Message::Binary(vec![]);
    }
    if cbor.len() >= COMPRESS_THRESHOLD {
        if let Ok(compressed) = zstd::encode_all(cbor.as_slice(), ZSTD_LEVEL) {
            if compressed.len() + 1 < cbor.len() {
                let mut out = Vec::with_capacity(1 + compressed.len());
                out.push(ZSTD_MAGIC);
                out.extend_from_slice(&compressed);
                return Message::Binary(out);
            }
        }
    }
    Message::Binary(cbor)
}

fn decode_client(bytes: &[u8]) -> Result<ClientMsg> {
    if bytes.first() == Some(&ZSTD_MAGIC) {
        let decompressed = zstd::decode_all(&bytes[1..]).context("zstd decompress ClientMsg")?;
        ciborium::from_reader(decompressed.as_slice()).context("cbor decode ClientMsg")
    } else {
        ciborium::from_reader(bytes).context("cbor decode ClientMsg")
    }
}
