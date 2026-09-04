use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use tokio::sync::broadcast;
use tracing::{info, warn};

const MAX_HISTORY_BYTES: usize = 128 * 1024; // 128 KB scrollback history
const BROADCAST_CAPACITY: usize = 512;

pub struct PtyManager {
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
    tx: broadcast::Sender<String>,
    scrollback: Arc<Mutex<VecDeque<u8>>>,
    is_alive: Arc<AtomicBool>,
}

impl PtyManager {
    /// Spawns a command in an interactive pseudo-terminal.
    pub fn spawn(
        program: &str,
        args: &[String],
        cwd: Option<&Path>,
        env_vars: &[(&str, &str)],
        rows: u16,
        cols: u16,
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

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("Failed to spawn command '{program}': {e}"))?;

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("Failed to take PTY writer: {e}"))?;

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("Failed to clone PTY reader: {e}"))?;

        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        let scrollback = Arc::new(Mutex::new(VecDeque::with_capacity(MAX_HISTORY_BYTES)));
        let is_alive = Arc::new(AtomicBool::new(true));

        let tx_reader = tx.clone();
        let scrollback_reader = scrollback.clone();
        let is_alive_reader = is_alive.clone();

        // Spawn a background thread to read output continuously from the PTY
        thread::Builder::new()
            .name("pty-reader".into())
            .spawn(move || {
                let mut buffer = [0u8; 4096];
                loop {
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
                            let text = String::from_utf8_lossy(chunk).to_string();
                            let _ = tx_reader.send(text);
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
            tx,
            scrollback,
            is_alive,
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
    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }

    /// Returns accumulated history from the scrollback buffer.
    pub fn get_history(&self) -> String {
        let sb = match self.scrollback.lock() {
            Ok(sb) => sb,
            Err(_) => return String::new(),
        };
        let bytes: Vec<u8> = sb.iter().copied().collect();
        String::from_utf8_lossy(&bytes).to_string()
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

    /// Terminates the running subprocess safely.
    pub fn kill(&self) -> Result<(), String> {
        self.is_alive.store(false, Ordering::SeqCst);
        let mut child = self.child.lock().map_err(|e| e.to_string())?;
        child.kill().map_err(|e| format!("Failed to kill PTY child: {}", e))
    }
}

impl Drop for PtyManager {
    fn drop(&mut self) {
        let _ = self.kill();
    }
}
