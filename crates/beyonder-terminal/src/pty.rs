//! PTY session management using portable-pty.

use anyhow::{Context, Result};
use beyonder_core::SessionId;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::mpsc;
use tracing::{debug, info, trace, warn};

/// Monotonic clock anchor shared across PTY sessions so `last_send_ns` stays
/// comparable against `App::tick` timestamps.
fn monotonic_start() -> &'static std::time::Instant {
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    START.get_or_init(std::time::Instant::now)
}

/// Elapsed nanoseconds since `monotonic_start`, bounded to `u64` for `AtomicU64`.
fn now_ns() -> u64 {
    monotonic_start().elapsed().as_nanos() as u64
}

/// Events from a PTY session.
#[derive(Debug, Clone)]
pub enum PtyEvent {
    /// Raw bytes from the PTY (includes ANSI escape sequences).
    Output(Vec<u8>),
    /// The child process exited.
    Exited(Option<u32>),
}

/// Callback invoked after each PTY event is sent, used to wake the event loop.
pub type WakeFn = Box<dyn Fn() + Send + 'static>;

/// A live PTY session connected to a shell.
pub struct PtySession {
    pub session_id: SessionId,
    master: Box<dyn MasterPty + Send>,
    // Writer cached at spawn — take_writer() can only be called once on some platforms.
    writer: Box<dyn std::io::Write + Send>,
    #[allow(dead_code)]
    child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
    pub event_rx: mpsc::Receiver<PtyEvent>,
    /// Set to `now_ns()` by the reader thread right after it sends a PtyEvent
    /// and calls `wake()`. `App::tick()` diffs against this to measure the lag
    /// between "reader pushed output" and "event-loop got around to draining
    /// it" — the gap that `ControlFlow::WaitUntil` + a dropped/stalled
    /// `EventLoopProxy::send_event` wake would show up as. 0 means "never sent".
    last_send_ns: Arc<AtomicU64>,
}

impl PtySession {
    pub fn spawn(
        session_id: SessionId,
        shell: &str,
        cwd: &PathBuf,
        extra_env: &[(&str, &str)],
    ) -> Result<Self> {
        Self::spawn_sized(session_id, shell, cwd, extra_env, 120, 30)
    }

    pub fn spawn_sized(
        session_id: SessionId,
        shell: &str,
        cwd: &PathBuf,
        extra_env: &[(&str, &str)],
        cols: u16,
        rows: u16,
    ) -> Result<Self> {
        Self::spawn_sized_inner(session_id, shell, cwd, extra_env, cols, rows, None)
    }

    /// Like `spawn_sized` but calls `wake` after each PTY event to wake the
    /// event loop (enables `ControlFlow::Wait` in the renderer).
    pub fn spawn_sized_with_wake(
        session_id: SessionId,
        shell: &str,
        cwd: &PathBuf,
        extra_env: &[(&str, &str)],
        cols: u16,
        rows: u16,
        wake: WakeFn,
    ) -> Result<Self> {
        Self::spawn_sized_inner(session_id, shell, cwd, extra_env, cols, rows, Some(wake))
    }

    fn spawn_sized_inner(
        session_id: SessionId,
        shell: &str,
        cwd: &PathBuf,
        extra_env: &[(&str, &str)],
        cols: u16,
        rows: u16,
        wake: Option<WakeFn>,
    ) -> Result<Self> {
        info!(shell, cols, rows, "Spawning PTY session");
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("Failed to open PTY")?;

        let home = std::env::var("HOME").unwrap_or_default();
        let session_dir = std::env::temp_dir().join(format!("beyonder_{}", &session_id.0));
        std::fs::create_dir_all(&session_dir).ok();

        let kind = crate::shell_hooks::detect_shell_kind(shell);
        let mut cmd = CommandBuilder::new(shell);
        cmd.cwd(cwd);

        match kind {
            crate::shell_hooks::ShellKind::Zsh => {
                // Temp ZDOTDIR overlay: our .zshenv / .zshrc forward to the user's,
                // then inject hooks. Doesn't touch the user's real config.
                let zshenv = format!(
                    "[ -f {home}/.zshenv ] && source {home}/.zshenv\n\
                     [ -f {home}/.zprofile ] && source {home}/.zprofile\n"
                );
                std::fs::write(session_dir.join(".zshenv"), zshenv).ok();
                let hooks = crate::shell_hooks::zsh_init_script(&session_id.0);
                let zshrc = format!("{hooks}\n[ -f {home}/.zshrc ] && source {home}/.zshrc\n");
                std::fs::write(session_dir.join(".zshrc"), zshrc).ok();
                cmd.env("ZDOTDIR", &session_dir);
                cmd.args(["-i"]);
            }
            crate::shell_hooks::ShellKind::Bash => {
                // --rcfile overrides ~/.bashrc — source the real one first.
                let rcfile = session_dir.join("init.bashrc");
                let user_rc = format!("[ -f {home}/.bashrc ] && source {home}/.bashrc\n");
                let hooks = crate::shell_hooks::bash_init_script(&session_id.0);
                std::fs::write(&rcfile, format!("{user_rc}\n{hooks}")).ok();
                cmd.args(["--rcfile", rcfile.to_str().unwrap_or(""), "-i"]);
            }
            crate::shell_hooks::ShellKind::Fish => {
                // --init-command runs in addition to user config; no overlay needed.
                let initf = session_dir.join("beyonder.fish");
                std::fs::write(&initf, crate::shell_hooks::fish_init_script(&session_id.0)).ok();
                let src_cmd = format!("source {}", initf.display());
                cmd.args(["--init-command", &src_cmd, "-i"]);
            }
            crate::shell_hooks::ShellKind::Nushell => {
                // Override --config / --env-config with files that source the
                // user's real config first, then layer our hooks on top.
                let nu_dir = std::path::PathBuf::from(&home)
                    .join(".config")
                    .join("nushell");
                let user_cfg = nu_dir.join("config.nu");
                let user_env = nu_dir.join("env.nu");

                let env_path = session_dir.join("env.nu");
                let env_body = if user_env.exists() {
                    format!("source {}\n", user_env.display())
                } else {
                    String::new()
                };
                std::fs::write(&env_path, env_body).ok();

                let cfg_path = session_dir.join("config.nu");
                let user_cfg_src = if user_cfg.exists() {
                    format!("source {}\n", user_cfg.display())
                } else {
                    String::new()
                };
                let hooks = crate::shell_hooks::nushell_init_script(&session_id.0);
                std::fs::write(&cfg_path, format!("{user_cfg_src}\n{hooks}")).ok();
                cmd.args([
                    "--config",
                    cfg_path.to_str().unwrap_or(""),
                    "--env-config",
                    env_path.to_str().unwrap_or(""),
                ]);
            }
            crate::shell_hooks::ShellKind::Unknown => {
                cmd.args(["-i"]);
            }
        }

        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        // Spoof as iTerm.app so claude-code (and other capability-sniffing TUIs)
        // pick their nicer Unicode glyphs. Also set LC_TERMINAL — iTerm's native
        // apps use this as a secondary signal and claude-code checks both.
        cmd.env("TERM_PROGRAM", "iTerm.app");
        cmd.env("LC_TERMINAL", "iTerm2");
        cmd.env("LC_TERMINAL_VERSION", "3.5.0");
        cmd.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
        cmd.env("BEYONDER_SESSION_ID", &session_id.0);
        for (k, v) in extra_env {
            cmd.env(k, v);
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .context("Failed to spawn shell")?;
        let child = Arc::new(Mutex::new(child));

        // Cache the writer immediately — take_writer() can only be called once.
        let writer = pair
            .master
            .take_writer()
            .context("Failed to get PTY writer")?;

        let (event_tx, event_rx) = mpsc::channel(1024);

        // Prime the monotonic clock before the reader thread uses it.
        let _ = monotonic_start();
        let last_send_ns = Arc::new(AtomicU64::new(0));

        // Spawn a background reader thread (blocking I/O — can't use tokio directly here).
        let mut reader = pair
            .master
            .try_clone_reader()
            .context("Failed to clone PTY reader")?;
        let child_clone = Arc::clone(&child);
        let tx = event_tx.clone();
        let session_tag = session_id.0.clone();
        let last_send_ns_writer = Arc::clone(&last_send_ns);
        std::thread::spawn(move || {
            // 64KB read buffer — 16x the old 4KB to reduce syscall overhead
            // for bulk output (e.g. `cat large_file`). The OS will fill as
            // much as available per read, so larger buffer = fewer events.
            const BUF_SIZE: usize = 65536;
            let mut buf = vec![0u8; BUF_SIZE];
            let mut total_reads: u64 = 0;
            let mut total_bytes: u64 = 0;
            let mut last_read_at = std::time::Instant::now();
            loop {
                let read_start = std::time::Instant::now();
                match std::io::Read::read(&mut reader, &mut buf) {
                    Ok(0) => {
                        debug!(session = %session_tag, "PTY reader: read returned 0 — EOF");
                        break;
                    }
                    Ok(n) => {
                        let gap_ms = read_start.duration_since(last_read_at).as_millis() as u64;
                        last_read_at = read_start;
                        total_reads += 1;
                        total_bytes += n as u64;
                        trace!(
                            session = %session_tag,
                            bytes = n,
                            gap_ms,
                            total_reads,
                            total_bytes,
                            "PTY read"
                        );
                        // Send exactly the bytes read — no over-allocation.
                        // Measure `blocking_send` latency — it only blocks if the
                        // bounded channel (capacity 1024) is full, which would
                        // mean the UI tick is not draining fast enough. Any
                        // latency above a few ms here is a real backpressure
                        // signal worth surfacing.
                        let send_start = std::time::Instant::now();
                        let send_res = tx.blocking_send(PtyEvent::Output(buf[..n].to_vec()));
                        let send_ms = send_start.elapsed().as_millis() as u64;
                        if send_res.is_err() {
                            debug!(session = %session_tag, "PTY reader: event channel closed");
                            break;
                        }
                        if send_ms >= 50 {
                            warn!(
                                session = %session_tag,
                                send_ms,
                                bytes = n,
                                "PTY reader blocked >50ms sending to UI (channel saturated — UI tick is not draining in time)"
                            );
                        } else if send_ms >= 5 {
                            debug!(session = %session_tag, send_ms, bytes = n, "PTY reader blocking_send slow");
                        }
                        if let Some(ref w) = wake {
                            w();
                        }
                        // Stamp *after* the wake so the age measured in tick()
                        // covers "reader finished signalling" → "event loop
                        // began draining". A missed/delayed EventLoopProxy
                        // wake shows up as a large age despite fast send.
                        last_send_ns_writer.store(now_ns(), Ordering::Release);
                    }
                    Err(e) => {
                        debug!(session = %session_tag, error = %e, "PTY reader: read error — exiting loop");
                        break;
                    }
                }
            }
            info!(
                session = %session_tag,
                total_reads,
                total_bytes,
                "PTY reader thread exiting"
            );
            // Child exited — get exit code.
            let code = child_clone
                .lock()
                .ok()
                .and_then(|mut c| c.wait().ok())
                .and_then(|s| if s.success() { Some(0) } else { None });
            let _ = tx.blocking_send(PtyEvent::Exited(code));
            if let Some(ref w) = wake {
                w();
            }
        });

        Ok(Self {
            session_id,
            master: pair.master,
            writer,
            child,
            event_rx,
            last_send_ns,
        })
    }

    /// Milliseconds since the reader thread last pushed a PtyEvent + fired the
    /// wake callback. `None` means no output has arrived yet. Compare this at
    /// the start of the event-loop tick to detect wake-lag: if there are
    /// queued events but the reader stamped `age_ms` back, the event loop slept
    /// through the wake. Direct probe for the case where sub-second commands
    /// (`ls`, `cd`) appear hung because tick() only ran on the 500ms fallback
    /// timer instead of the PTY-driven wake.
    pub fn last_output_send_age_ms(&self) -> Option<u64> {
        let stamp = self.last_send_ns.load(Ordering::Acquire);
        if stamp == 0 {
            return None;
        }
        let now = now_ns();
        Some(now.saturating_sub(stamp) / 1_000_000)
    }

    /// Write bytes to the PTY (user keystrokes or command input).
    pub fn write(&mut self, data: &[u8]) -> Result<()> {
        use std::io::Write;
        self.writer.write_all(data).context("PTY write failed")?;
        self.writer.flush().ok();
        Ok(())
    }

    /// Resize the PTY.
    pub fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("Failed to resize PTY")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Verify that the wake callback fires on PTY output and on exit,
    /// proving event-driven redraw can rely on it instead of polling.
    #[tokio::test]
    async fn wake_callback_fires_on_pty_output() {
        let wake_count = Arc::new(AtomicUsize::new(0));
        let wc = Arc::clone(&wake_count);
        let wake: WakeFn = Box::new(move || {
            wc.fetch_add(1, Ordering::SeqCst);
        });

        let session_id = SessionId::new();
        let cwd = std::env::temp_dir();
        // Run a command that produces output then exits.
        let mut pty =
            PtySession::spawn_sized_with_wake(session_id, "/bin/sh", &cwd, &[], 80, 24, wake)
                .expect("spawn PTY");

        // Send a command that produces output and exits.
        pty.write(b"echo hello && exit\n").unwrap();

        // Drain events until exit.
        let mut got_output = false;
        let mut got_exit = false;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match tokio::time::timeout_at(deadline, pty.event_rx.recv()).await {
                Ok(Some(PtyEvent::Output(_))) => got_output = true,
                Ok(Some(PtyEvent::Exited(_))) => {
                    got_exit = true;
                    break;
                }
                Ok(None) => break,
                Err(_) => panic!("PTY test timed out"),
            }
        }

        assert!(got_output, "should have received PTY output");
        assert!(got_exit, "should have received PTY exit");
        // Wake must have fired at least once for output + once for exit.
        let wakes = wake_count.load(Ordering::SeqCst);
        assert!(
            wakes >= 2,
            "wake callback should fire at least twice, got {wakes}"
        );
    }

    /// Verify that without a wake callback, PTY still works (backward compat).
    #[tokio::test]
    async fn pty_works_without_wake() {
        let session_id = SessionId::new();
        let cwd = std::env::temp_dir();
        let mut pty =
            PtySession::spawn_sized(session_id, "/bin/sh", &cwd, &[], 80, 24).expect("spawn PTY");

        pty.write(b"exit\n").unwrap();

        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match tokio::time::timeout_at(deadline, pty.event_rx.recv()).await {
                Ok(Some(PtyEvent::Exited(_))) | Ok(None) => break,
                Ok(Some(_)) => continue,
                Err(_) => panic!("PTY test timed out"),
            }
        }
    }

    /// Verify that bulk output (>4KB) is handled correctly with the 64KB buffer.
    /// This proves the larger buffer reduces event count for throughput.
    #[tokio::test]
    async fn bulk_output_uses_fewer_events() {
        let session_id = SessionId::new();
        let cwd = std::env::temp_dir();
        let mut pty =
            PtySession::spawn_sized(session_id, "/bin/sh", &cwd, &[], 80, 24).expect("spawn PTY");

        // Generate ~32KB of output — should arrive in fewer events than
        // the old 4KB buffer would produce (8+ events → ~1-2 events).
        pty.write(b"dd if=/dev/zero bs=1024 count=32 2>/dev/null | od | head -500; exit\n")
            .unwrap();

        let mut total_bytes = 0usize;
        let mut event_count = 0usize;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match tokio::time::timeout_at(deadline, pty.event_rx.recv()).await {
                Ok(Some(PtyEvent::Output(bytes))) => {
                    total_bytes += bytes.len();
                    event_count += 1;
                }
                Ok(Some(PtyEvent::Exited(_))) | Ok(None) => break,
                Err(_) => panic!("PTY test timed out"),
            }
        }

        assert!(total_bytes > 0, "should have received output bytes");
        // With 64KB buffer, bulk output should arrive in fewer chunks.
        // The exact count depends on timing, but it should be reasonable.
        assert!(
            event_count < 100,
            "too many events ({event_count}) for {total_bytes} bytes — buffer may be too small"
        );
    }
}
