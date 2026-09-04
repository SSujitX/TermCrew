use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::session::AppState;

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
#[allow(dead_code)]
pub enum WsOutbound<'a> {
    #[serde(rename = "stdout")]
    Stdout { data: &'a str },
    #[serde(rename = "connected")]
    Connected { session_id: &'a str, data: &'a str },
    #[serde(rename = "pong")]
    Pong,
    #[serde(rename = "error")]
    Error { message: &'a str },
}

/// WebSocket upgrade handler for `/ws/:session_id`
pub async fn ws_handler(
    Path(session_id): Path<String>,
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, session_id, state))
}

async fn handle_socket(socket: WebSocket, session_id: String, state: AppState) {
    let pty = {
        let map = state.read().await;
        match map.get(&session_id) {
            Some(session) => session.pty.clone(),
            None => {
                warn!("WebSocket rejected: session '{}' not found", session_id);
                return;
            }
        }
    };

    info!(session_id = %session_id, "WebSocket client connected");

    let (mut sender, mut receiver) = socket.split();

    // Send history and connection confirmation
    let history = pty.get_history();
    let connected_payload = match serde_json::to_string(&WsOutbound::Connected {
        session_id: &session_id,
        data: &history,
    }) {
        Ok(json) => json,
        Err(_) => return,
    };

    if let Err(e) = sender.send(Message::Text(connected_payload)).await {
        warn!(session_id = %session_id, "Failed to send initial history: {}", e);
        return;
    }

    // Subscribe to PTY stdout broadcast
    let mut pty_rx = pty.subscribe();
    let pty_writer = pty.clone();

    // Pump PTY output -> WebSocket client
    let mut send_task = tokio::spawn(async move {
        while let Ok(chunk) = pty_rx.recv().await {
            let msg = match serde_json::to_string(&WsOutbound::Stdout { data: &chunk }) {
                Ok(j) => j,
                Err(_) => continue,
            };
            if sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    // Pump WebSocket client input -> PTY stdin / resize
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
                            WsInbound::Ping => {
                                // Handled automatically by WebSocket ping/pong
                            }
                        }
                    } else {
                        // Fallback: raw text as input
                        let _ = pty_writer.write_input(text.as_bytes());
                    }
                }
                Ok(Message::Binary(bin)) => {
                    let _ = pty_writer.write_input(&bin);
                }
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
    });

    // If either task completes, abort the other
    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    }

    info!(session_id = %session_id, "WebSocket session disconnected");
}
