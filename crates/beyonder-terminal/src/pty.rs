//! PTY session management using portable-pty.

use anyhow::{Context, Result};
use beyonder_core::SessionId;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::{error, info};

/// Events from a PTY session.
#[derive(Debug, Clone)]
pub enum PtyEvent {
    /// Raw bytes from the PTY (includes ANSI escape sequences).
    Output(Vec<u8>),
    /// The child process exited.
    Exited(Option<u32>),
}

/// A live PTY session connected to a shell.
pub struct PtySession {
    pub session_id: SessionId,
    master: Box<dyn MasterPty + Send>,
    // Writer cached at spawn — take_writer() can only be called once on some platforms.
    writer: Box<dyn std::io::Write + Send>,
    child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
    pub event_rx: mpsc::Receiver<PtyEvent>,
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

        // Write Beyonder shell integration to a temp ZDOTDIR so it's sourced
        // automatically when zsh starts, without overriding the user's config.
        let home = std::env::var("HOME").unwrap_or_default();
        let zdotdir = std::env::temp_dir().join(format!("beyonder_{}", &session_id.0));
        std::fs::create_dir_all(&zdotdir).ok();

        // .zshenv is always sourced — forward the real one, then source .zprofile
        // so PATH includes binaries installed via npm/nvm/brew etc.
        let zshenv = format!(
            "[ -f {home}/.zshenv ] && source {home}/.zshenv\n\
             [ -f {home}/.zprofile ] && source {home}/.zprofile\n"
        );
        std::fs::write(zdotdir.join(".zshenv"), zshenv).ok();

        // .zshrc: Beyonder hooks first, then the user's real .zshrc.
        use crate::shell_hooks::zsh_init_script;
        let hooks = zsh_init_script(&session_id.0);
        let zshrc = format!("{hooks}\n[ -f {home}/.zshrc ] && source {home}/.zshrc\n");
        std::fs::write(zdotdir.join(".zshrc"), zshrc).ok();

        let mut cmd = CommandBuilder::new(shell);
        cmd.args(&["-i"]); // force interactive mode so .zshrc is sourced
        cmd.cwd(cwd);

        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "Beyond");
        cmd.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
        cmd.env("ZDOTDIR", &zdotdir);
        cmd.env("BEYONDER_SESSION_ID", &session_id.0);
        for (k, v) in extra_env {
            cmd.env(k, v);
        }

        let child = pair.slave.spawn_command(cmd).context("Failed to spawn shell")?;
        let child = Arc::new(Mutex::new(child));

        // Cache the writer immediately — take_writer() can only be called once.
        let writer = pair.master.take_writer().context("Failed to get PTY writer")?;

        let (event_tx, event_rx) = mpsc::channel(1024);

        // Spawn a background reader thread (blocking I/O — can't use tokio directly here).
        let mut reader = pair
            .master
            .try_clone_reader()
            .context("Failed to clone PTY reader")?;
        let child_clone = Arc::clone(&child);
        let tx = event_tx.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match std::io::Read::read(&mut reader, &mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let _ = tx.blocking_send(PtyEvent::Output(buf[..n].to_vec()));
                    }
                    Err(_) => break,
                }
            }
            // Child exited — get exit code.
            let code = child_clone
                .lock()
                .ok()
                .and_then(|mut c| c.wait().ok())
                .and_then(|s| if s.success() { Some(0) } else { None });
            let _ = tx.blocking_send(PtyEvent::Exited(code));
        });

        Ok(Self {
            session_id,
            master: pair.master,
            writer,
            child,
            event_rx,
        })
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
            .resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .context("Failed to resize PTY")
    }
}
