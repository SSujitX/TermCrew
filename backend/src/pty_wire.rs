/// ttyd-style opcode frames. First byte is the command; the rest is payload.
pub const OP_DATA: u8 = 0x00;
pub const OP_RESIZE: u8 = 0x01;
pub const OP_PAUSE: u8 = 0x02;
pub const OP_RESUME: u8 = 0x03;
pub const OP_ACK: u8 = 0x04;
pub const OP_EXIT: u8 = 0x05;
pub const OP_SETUP: u8 = 0x06;

pub const SETUP_SENTINEL_OK: &str = "__MA_SETUP__:ok";
pub const SETUP_SENTINEL_FAIL: &str = "__MA_SETUP__:fail";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientFrame {
    Input(Vec<u8>),
    Resize { cols: u16, rows: u16 },
    Pause,
    Resume,
    Ack(u32),
}

pub fn frame_data(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + bytes.len());
    out.push(OP_DATA);
    out.extend_from_slice(bytes);
    out
}

pub fn frame_exit(code: i32) -> Vec<u8> {
    let mut out = Vec::with_capacity(5);
    out.push(OP_EXIT);
    out.extend_from_slice(&code.to_le_bytes());
    out
}

pub fn frame_setup(ok: bool) -> Vec<u8> {
    vec![OP_SETUP, u8::from(ok)]
}

pub fn parse_client(bytes: &[u8]) -> Option<ClientFrame> {
    let op = *bytes.first()?;
    let payload = &bytes[1..];
    match op {
        OP_DATA => Some(ClientFrame::Input(payload.to_vec())),
        OP_RESIZE if payload.len() >= 4 => {
            let cols = u16::from_le_bytes([payload[0], payload[1]]);
            let rows = u16::from_le_bytes([payload[2], payload[3]]);
            Some(ClientFrame::Resize { cols, rows })
        }
        OP_PAUSE => Some(ClientFrame::Pause),
        OP_RESUME => Some(ClientFrame::Resume),
        OP_ACK if payload.len() >= 4 => {
            Some(ClientFrame::Ack(u32::from_le_bytes([
                payload[0], payload[1], payload[2], payload[3],
            ])))
        }
        _ => None,
    }
}

pub fn strip_setup_sentinel(text: &str) -> (String, Option<bool>) {
    let mut result = None;
    let mut cleaned = text.to_string();
    if cleaned.contains(SETUP_SENTINEL_OK) {
        result = Some(true);
        cleaned = cleaned.replace(SETUP_SENTINEL_OK, "");
    }
    if cleaned.contains(SETUP_SENTINEL_FAIL) {
        result = Some(false);
        cleaned = cleaned.replace(SETUP_SENTINEL_FAIL, "");
    }
    (cleaned, result)
}

/// Scrollback from a setup shell that already finished. Strip the sentinel
/// and return the UI result so reconnect/replay still updates the Agents list.
pub fn replay_setup_history(history: &[u8]) -> (Vec<u8>, Option<bool>) {
    let text = String::from_utf8_lossy(history);
    let (cleaned, result) = strip_setup_sentinel(&text);
    (cleaned.into_bytes(), result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_roundtrip() {
        let framed = frame_data(b"hello");
        assert_eq!(framed[0], OP_DATA);
        let parsed = parse_client(&framed).unwrap();
        assert_eq!(parsed, ClientFrame::Input(b"hello".to_vec()));
    }

    #[test]
    fn resize_and_ack_roundtrip() {
        let mut resize = vec![OP_RESIZE];
        resize.extend_from_slice(&80u16.to_le_bytes());
        resize.extend_from_slice(&24u16.to_le_bytes());
        assert_eq!(
            parse_client(&resize),
            Some(ClientFrame::Resize { cols: 80, rows: 24 })
        );
        let mut ack = vec![OP_ACK];
        ack.extend_from_slice(&4096u32.to_le_bytes());
        assert_eq!(parse_client(&ack), Some(ClientFrame::Ack(4096)));
    }

    #[test]
    fn strips_setup_sentinel() {
        let (text, ok) = strip_setup_sentinel("Installed successfully.\r\n__MA_SETUP__:ok\r\n");
        assert_eq!(ok, Some(true));
        assert!(!text.contains("__MA_SETUP__"));
        assert!(text.contains("Installed successfully."));
    }

    #[test]
    fn replay_history_still_reports_setup_result() {
        let history = b"Removing Antigravity CLI...\r\n\
Remove-Item -Path \"$env:LOCALAPPDATA\\agy\" -Recurse -Force\r\n\
Removed successfully.\r\n__MA_SETUP__:ok\r\n";
        let (cleaned, ok) = replay_setup_history(history);
        assert_eq!(ok, Some(true));
        let text = String::from_utf8(cleaned).unwrap();
        assert!(text.contains("Removed successfully."));
        assert!(text.contains("Remove-Item"));
        assert!(!text.contains("__MA_SETUP__"));
    }
}
