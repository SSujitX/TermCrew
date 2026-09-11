use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

use crate::persist;
use crate::pty_wire::{
    self, frame_data, frame_exit, frame_setup, parse_client, strip_setup_sentinel, ClientFrame,
};
use crate::session::AppState;

const FLOW_HIGH: usize = 256 * 1024;
const FLOW_LOW: usize = 32 * 1024;
const MAX_COALESCE_BYTES: usize = 64 * 1024;
/// Wait for the browser to report real cols/rows before dumping scrollback.
/// Otherwise TUI agents (Ink/Kilo/etc.) replay absolute-positioned frames at 80×24.
const RESIZE_WAIT: Duration = Duration::from_millis(600);
const RESIZE_SETTLE: Duration = Duration::from_millis(120);

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum WsInbound {
    #[serde(rename = "input")]
    Input { data: String },
    #[serde(rename = "resize")]
    Resize { cols: u16, rows: u16 },
    #[serde(rename = "ping")]
    Ping,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum WsOutbound<'a> {
    #[serde(rename = "connected")]
    Connected { session_id: &'a str },
    #[serde(rename = "error")]
    Error { message: &'a str },
}

pub async fn ws_handler(
    Path(session_id): Path<String>,
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, session_id, state))
}

async fn handle_socket(mut socket: WebSocket, session_id: String, state: AppState) {
    let pty = {
        let map = state.read().await;
        match map.get(&session_id) {
            None => {
                drop(map);
                warn!("WebSocket rejected: session '{}' not found", session_id);
                let message = format!(
                    "Session '{session_id}' not found on the backend. It may have been closed."
                );
                if let Ok(json) = serde_json::to_string(&WsOutbound::Error { message: &message }) {
                    let _ = socket.send(Message::Text(json)).await;
                }
                let _ = socket.send(Message::Close(None)).await;
                return;
            }
            Some(session) => session.pty.clone(),
        }
    };

    let Some(pty) = pty else {
        handle_parked_socket(socket, session_id).await;
        return;
    };

    info!(session_id = %session_id, "WebSocket client connected");

    let connected_payload = match serde_json::to_string(&WsOutbound::Connected {
        session_id: &session_id,
    }) {
        Ok(json) => json,
        Err(_) => return,
    };
    if socket
        .send(Message::Text(connected_payload))
        .await
        .is_err()
    {
        return;
    }

    // Apply the client's real size before scrollback so TUIs can SIGWINCH-redraw
    // into the buffer; then dump history (typically ends with the fresh frame).
    let mut pending_input: Vec<Vec<u8>> = Vec::new();
    let mut got_resize = false;
    let deadline = tokio::time::Instant::now() + RESIZE_WAIT;
    while tokio::time::Instant::now() < deadline {
        let wait = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(wait, socket.recv()).await {
            Ok(Some(Ok(msg))) => match msg {
                Message::Close(_) => {
                    info!(session_id = %session_id, "WebSocket closed before resize");
                    return;
                }
                Message::Text(text) => {
                    if let Ok(inbound) = serde_json::from_str::<WsInbound>(&text) {
                        match inbound {
                            WsInbound::Resize { cols, rows } => {
                                let _ = pty.resize(cols, rows);
                                got_resize = true;
                                break;
                            }
                            WsInbound::Input { data } => {
                                pending_input.push(data.into_bytes());
                            }
                            WsInbound::Ping => {}
                        }
                    }
                }
                Message::Binary(bin) => match parse_client(&bin) {
                    Some(ClientFrame::Resize { cols, rows }) => {
                        let _ = pty.resize(cols, rows);
                        got_resize = true;
                        break;
                    }
                    Some(ClientFrame::Input(data)) => pending_input.push(data),
                    Some(ClientFrame::Pause) => pty.set_paused(true),
                    Some(ClientFrame::Resume) => pty.set_paused(false),
                    Some(ClientFrame::Ack(_)) | None => {}
                },
                _ => {}
            },
            Ok(Some(Err(e))) => {
                warn!(session_id = %session_id, "WebSocket error before resize: {e}");
                return;
            }
            Ok(None) => return,
            Err(_) => break,
        }
    }

    if got_resize {
        tokio::time::sleep(RESIZE_SETTLE).await;
    }

    let history = pty.get_history();
    if !history.is_empty() {
        let (cleaned, setup) = pty_wire::replay_setup_history(&history);
        if !cleaned.is_empty()
            && socket
                .send(Message::Binary(frame_data(&cleaned)))
                .await
                .is_err()
        {
            return;
        }
        if let Some(ok) = setup {
            if socket.send(Message::Binary(frame_setup(ok))).await.is_err() {
                return;
            }
        }
    }

    for chunk in pending_input {
        let _ = pty.write_input(&chunk);
    }

    let (mut sender, mut receiver) = socket.split();

    let mut pty_rx = pty.subscribe();
    let pty_writer = pty.clone();
    let flow_pty = pty.clone();
    let unacked = Arc::new(AtomicUsize::new(0));
    let unacked_send = unacked.clone();
    let unacked_recv = unacked.clone();

    let mut send_task = tokio::spawn(async move {
        let mut alive_tick = tokio::time::interval(Duration::from_millis(500));
        alive_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            let mut chunk = tokio::select! {
                result = pty_rx.recv() => match result {
                    Ok(c) => c,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        let _ = sender.send(Message::Binary(frame_exit(0))).await;
                        break;
                    }
                },
                _ = alive_tick.tick() => {
                    if !flow_pty.is_alive() {
                        let _ = sender.send(Message::Binary(frame_exit(0))).await;
                        break;
                    }
                    continue;
                }
            };
            while chunk.len() < MAX_COALESCE_BYTES {
                match pty_rx.try_recv() {
                    Ok(more) => chunk.extend_from_slice(&more),
                    Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
                    Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::TryRecvError::Closed) => break,
                }
            }

            if !chunk.is_empty() {
                let text = String::from_utf8_lossy(&chunk);
                let (cleaned, setup) = strip_setup_sentinel(&text);
                if let Some(ok) = setup {
                    let _ = sender.send(Message::Binary(frame_setup(ok))).await;
                }
                if !cleaned.is_empty() {
                    let bytes = cleaned.into_bytes();
                    unacked_send.fetch_add(bytes.len(), Ordering::SeqCst);
                    if sender
                        .send(Message::Binary(frame_data(&bytes)))
                        .await
                        .is_err()
                    {
                        break;
                    }
                    if unacked_send.load(Ordering::SeqCst) > FLOW_HIGH {
                        flow_pty.set_paused(true);
                    }
                }
            }

            if !flow_pty.is_alive() {
                let _ = sender.send(Message::Binary(frame_exit(0))).await;
                break;
            }
        }
        flow_pty.set_paused(false);
    });

    let session_id_clone = session_id.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(result) = receiver.next().await {
            match result {
                Ok(Message::Text(text)) => {
                    if let Ok(inbound) = serde_json::from_str::<WsInbound>(&text) {
                        match inbound {
                            WsInbound::Input { data } => {
                                let _ = pty_writer.write_input(data.as_bytes());
                            }
                            WsInbound::Resize { cols, rows } => {
                                let _ = pty_writer.resize(cols, rows);
                            }
                            WsInbound::Ping => {}
                        }
                    }
                }
                Ok(Message::Binary(bin)) => match parse_client(&bin) {
                    Some(ClientFrame::Input(data)) => {
                        let _ = pty_writer.write_input(&data);
                    }
                    Some(ClientFrame::Resize { cols, rows }) => {
                        let _ = pty_writer.resize(cols, rows);
                    }
                    Some(ClientFrame::Pause) => pty_writer.set_paused(true),
                    Some(ClientFrame::Resume) => pty_writer.set_paused(false),
                    Some(ClientFrame::Ack(n)) => {
                        let prev = unacked_recv.load(Ordering::SeqCst);
                        unacked_recv.store(prev.saturating_sub(n as usize), Ordering::SeqCst);
                        if unacked_recv.load(Ordering::SeqCst) < FLOW_LOW {
                            pty_writer.set_paused(false);
                        }
                    }
                    None => {
                        if !bin.is_empty() && bin[0] != pty_wire::OP_DATA {
                            let _ = pty_writer.write_input(&bin);
                        }
                    }
                },
                Ok(Message::Close(_)) => {
                    info!(session_id = %session_id_clone, "WebSocket closed by client");
                    break;
                }
                Err(e) => {
                    warn!(session_id = %session_id_clone, "WebSocket receive error: {}", e);
                    break;
                }
                _ => {}
            }
        }
        pty_writer.set_paused(false);
    });

    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    }

    info!(session_id = %session_id, "WebSocket session disconnected");
}

/// Parked session after backend restart: replay disk scrollback and tell the UI to relaunch.
async fn handle_parked_socket(mut socket: WebSocket, session_id: String) {
    info!(session_id = %session_id, "WebSocket connected to parked session");

    let connected_payload = match serde_json::to_string(&WsOutbound::Connected {
        session_id: &session_id,
    }) {
        Ok(json) => json,
        Err(_) => return,
    };
    if socket
        .send(Message::Text(connected_payload))
        .await
        .is_err()
    {
        return;
    }

    // Drain an optional early resize so the client fit path stays consistent.
    let deadline = tokio::time::Instant::now() + RESIZE_WAIT;
    while tokio::time::Instant::now() < deadline {
        let wait = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(wait, socket.recv()).await {
            Ok(Some(Ok(Message::Close(_)))) | Ok(None) => return,
            Ok(Some(Ok(Message::Binary(bin)))) => {
                if matches!(parse_client(&bin), Some(ClientFrame::Resize { .. })) {
                    break;
                }
            }
            Ok(Some(Ok(Message::Text(text)))) => {
                if matches!(
                    serde_json::from_str::<WsInbound>(&text),
                    Ok(WsInbound::Resize { .. })
                ) {
                    break;
                }
            }
            Ok(Some(Err(_))) => return,
            Err(_) => break,
            _ => {}
        }
    }

    let history = persist::load_scrollback(&session_id);
    if !history.is_empty() {
        let (cleaned, _) = pty_wire::replay_setup_history(&history);
        if !cleaned.is_empty() {
            let _ = socket.send(Message::Binary(frame_data(&cleaned))).await;
        }
    }

    let banner = b"\r\n\x1b[33m[TermCrew]\x1b[0m Session is parked (backend restarted). \
Press Restart to relaunch this agent.\r\n";
    let _ = socket.send(Message::Binary(frame_data(banner))).await;
    let _ = socket.send(Message::Binary(frame_exit(0))).await;

    // Keep the socket open until the client closes (pane still shows history).
    while let Some(Ok(msg)) = socket.recv().await {
        if matches!(msg, Message::Close(_)) {
            break;
        }
    }
}
