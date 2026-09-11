use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use tokio::sync::broadcast;
use tracing::{info, warn};

use crate::proc_guard::ProcessGuard;

const MAX_HISTORY_BYTES: usize = 128 * 1024; // 128 KB scrollback history
const BROADCAST_CAPACITY: usize = 8192;
const PTY_READ_BUF: usize = 32 * 1024;

pub struct PtyManager {
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
    /// Kill-tree guard (Job Object / process group) for the whole agent tree.
    guard: Option<ProcessGuard>,
    tx: broadcast::Sender<Vec<u8>>,
    scrollback: Arc<Mutex<VecDeque<u8>>>,
    is_alive: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
}

impl PtyManager {
    /// Spawns a command in an interactive pseudo-terminal.
    /// `seed_history` (if any) is loaded into the scrollback ring before live output.
    pub fn spawn(
        program: &str,
        args: &[String],
        cwd: Option<&Path>,
        env_vars: &[(&str, &str)],
        rows: u16,
        cols: u16,
        seed_history: Option<Vec<u8>>,
    ) -> Result<Arc<Self>, String> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("Failed to open PTY: {}", e))?;

        let mut cmd = CommandBuilder::new(program);
        for arg in args {
            cmd.arg(arg);
        }
        if let Some(dir) = cwd {
            cmd.cwd(dir);
        }
        for (k, v) in env_vars {
            cmd.env(k, v);
        }
        // Cursor / CI often set NO_COLOR; we set FORCE_COLOR for truecolor PTYs.
        // Node warns and ignores NO_COLOR when both are present.
        cmd.env_remove("NO_COLOR");

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("Failed to spawn command '{program}': {e}"))?;

        // Attach the tree-kill guard immediately, before the child has a
        // chance to spawn grandchildren (node/python workers) that would
        // miss job / process-group membership.
        let guard = child.process_id().and_then(ProcessGuard::attach);

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("Failed to take PTY writer: {e}"))?;

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("Failed to clone PTY reader: {e}"))?;

        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        let mut initial = VecDeque::with_capacity(MAX_HISTORY_BYTES);
        if let Some(seed) = seed_history {
            let start = seed.len().saturating_sub(MAX_HISTORY_BYTES);
            initial.extend(seed[start..].iter().copied());
        }
        let scrollback = Arc::new(Mutex::new(initial));
        let is_alive = Arc::new(AtomicBool::new(true));
        let paused = Arc::new(AtomicBool::new(false));

        let tx_reader = tx.clone();
        let scrollback_reader = scrollback.clone();
        let is_alive_reader = is_alive.clone();
        let paused_reader = paused.clone();

        // Not reading while paused fills the OS PTY buffer and blocks the child
        // (ttyd / xterm flow-control model).
        thread::Builder::new()
            .name("pty-reader".into())
            .spawn(move || {
                let mut buffer = [0u8; PTY_READ_BUF];
                loop {
                    while paused_reader.load(Ordering::SeqCst) {
                        thread::sleep(std::time::Duration::from_millis(8));
                    }
                    match reader.read(&mut buffer) {
                        Ok(0) => {
                            info!("PTY output reached EOF");
                            break;
                        }
                        Ok(n) => {
                            let chunk = &buffer[..n];
                            {
                                if let Ok(mut sb) = scrollback_reader.lock() {
                                    sb.extend(chunk);
                                    while sb.len() > MAX_HISTORY_BYTES {
                                        sb.pop_front();
                                    }
                                }
                            }
                            let _ = tx_reader.send(chunk.to_vec());
                        }
                        Err(e) => {
                            warn!("PTY reader ended with error: {}", e);
                            break;
                        }
                    }
                }
                is_alive_reader.store(false, Ordering::SeqCst);
            })
            .map_err(|e| format!("Failed to spawn PTY reader thread: {e}"))?;

        let manager = Arc::new(Self {
            master: Arc::new(Mutex::new(pair.master)),
            writer: Arc::new(Mutex::new(writer)),
            child: Arc::new(Mutex::new(child)),
            guard,
            tx,
            scrollback,
            is_alive,
            paused,
        });

        Ok(manager)
    }

    /// Writes raw input into the PTY stdin.
    pub fn write_input(&self, data: &[u8]) -> Result<(), String> {
        let mut writer = self.writer.lock().map_err(|e| e.to_string())?;
        writer.write_all(data).map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Resizes the PTY terminal window.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        let master = self.master.lock().map_err(|e| e.to_string())?;
        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("Failed to resize PTY: {}", e))
    }

    /// Returns a new broadcast receiver for stdout streaming.
    pub fn subscribe(&self) -> broadcast::Receiver<Vec<u8>> {
        self.tx.subscribe()
    }

    /// Returns accumulated history from the scrollback buffer.
    pub fn get_history(&self) -> Vec<u8> {
        let sb = match self.scrollback.lock() {
            Ok(sb) => sb,
            Err(_) => return Vec::new(),
        };
        sb.iter().copied().collect()
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::SeqCst);
    }

    /// Checks if the PTY process is still running.
    pub fn is_alive(&self) -> bool {
        if !self.is_alive.load(Ordering::SeqCst) {
            return false;
        }
        if let Ok(mut child) = self.child.lock() {
            match child.try_wait() {
                Ok(Some(_status)) => {
                    self.is_alive.store(false, Ordering::SeqCst);
                    false
                }
                Ok(None) => true,
                Err(_) => {
                    self.is_alive.store(false, Ordering::SeqCst);
                    false
                }
            }
        } else {
            false
        }
    }

    /// Terminates the running subprocess and every process it spawned.
    pub fn kill(&self) -> Result<(), String> {
        self.is_alive.store(false, Ordering::SeqCst);
        if let Some(guard) = &self.guard {
            guard.kill();
        }
        let mut child = self.child.lock().map_err(|e| e.to_string())?;
        child.kill().map_err(|e| format!("Failed to kill PTY child: {}", e))
    }
}

impl Drop for PtyManager {
    fn drop(&mut self) {
        let _ = self.kill();
    }
}
