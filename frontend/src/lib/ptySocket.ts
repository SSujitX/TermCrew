export type PtyPayload = {
  type: string;
  data?: string;
  message?: string;
};

const OP_DATA = 0x00;
const OP_RESIZE = 0x01;
const OP_PAUSE = 0x02;
const OP_RESUME = 0x03;
const OP_ACK = 0x04;
const OP_EXIT = 0x05;
const OP_SETUP = 0x06;

const ACK_EVERY = 32 * 1024;

type Handlers = {
  onLive: (live: boolean) => void;
  onBytes: (bytes: Uint8Array, onParsed?: () => void) => void;
  onPayload: (payload: PtyPayload) => void;
  onOpen?: (ws: WebSocket) => void;
  onExit?: (code: number) => void;
  onSetupDone?: (ok: boolean) => void;
};

export function ptyWsUrl(sessionId: string): string {
  const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  if (import.meta.env.DEV) {
    return `${proto}//127.0.0.1:3001/ws/${sessionId}`;
  }
  return `${proto}//${window.location.host}/ws/${sessionId}`;
}

function u16le(n: number): Uint8Array {
  const b = new Uint8Array(2);
  new DataView(b.buffer).setUint16(0, n, true);
  return b;
}

function u32le(n: number): Uint8Array {
  const b = new Uint8Array(4);
  new DataView(b.buffer).setUint32(0, n, true);
  return b;
}

function concat(parts: Uint8Array[]): Uint8Array {
  const len = parts.reduce((n, p) => n + p.length, 0);
  const out = new Uint8Array(len);
  let o = 0;
  for (const p of parts) {
    out.set(p, o);
    o += p.length;
  }
  return out;
}

/** Opcode PTY socket: binary data/input, ACK flow control, setup/exit events. */
export function openPtySocket(sessionId: string, handlers: Handlers) {
  let disposed = false;
  let ws: WebSocket | null = null;
  let retryTimer: ReturnType<typeof setTimeout> | null = null;
  let attempt = 0;
  let pendingAck = 0;
  let paused = false;

  const sendRaw = (bytes: Uint8Array): boolean => {
    if (!ws || ws.readyState !== WebSocket.OPEN) return false;
    // All outgoing frames are exact views over their own ArrayBuffer.
    ws.send(bytes as Uint8Array<ArrayBuffer>);
    return true;
  };

  const send = (obj: unknown): boolean => {
    if (!ws || ws.readyState !== WebSocket.OPEN) return false;
    if (obj && typeof obj === 'object' && 'type' in obj) {
      const msg = obj as { type: string; data?: string; cols?: number; rows?: number };
      if (msg.type === 'input' && typeof msg.data === 'string') {
        return sendRaw(concat([new Uint8Array([OP_DATA]), new TextEncoder().encode(msg.data)]));
      }
      if (msg.type === 'resize' && typeof msg.cols === 'number' && typeof msg.rows === 'number') {
        return sendRaw(concat([new Uint8Array([OP_RESIZE]), u16le(msg.cols), u16le(msg.rows)]));
      }
    }
    ws.send(JSON.stringify(obj));
    return true;
  };

  const ackIfNeeded = (n: number) => {
    pendingAck += n;
    if (pendingAck >= ACK_EVERY) {
      sendRaw(concat([new Uint8Array([OP_ACK]), u32le(pendingAck)]));
      pendingAck = 0;
    }
  };

  const connect = () => {
    if (disposed) return;
    const next = new WebSocket(ptyWsUrl(sessionId));
    next.binaryType = 'arraybuffer';
    ws = next;

    next.onopen = () => {
      attempt = 0;
      pendingAck = 0;
      paused = false;
      handlers.onLive(true);
      handlers.onOpen?.(next);
    };

    next.onmessage = (event) => {
      if (event.data instanceof ArrayBuffer) {
        const bytes = new Uint8Array(event.data);
        if (bytes.length === 0) return;
        const op = bytes[0];
        const payload = bytes.subarray(1);
        if (op === OP_DATA) {
          handlers.onBytes(payload, () => ackIfNeeded(payload.length));
          return;
        }
        if (op === OP_EXIT) {
          const code = payload.length >= 4 ? new DataView(payload.buffer, payload.byteOffset, 4).getInt32(0, true) : 0;
          handlers.onExit?.(code);
          return;
        }
        if (op === OP_SETUP) {
          handlers.onSetupDone?.(payload[0] === 1);
          return;
        }
        handlers.onBytes(bytes);
        return;
      }
      if (typeof event.data === 'string') {
        try {
          handlers.onPayload(JSON.parse(event.data) as PtyPayload);
        } catch {
          handlers.onBytes(new TextEncoder().encode(event.data));
        }
      }
    };

    next.onerror = () => {
      handlers.onLive(false);
    };

    next.onclose = () => {
      handlers.onLive(false);
      if (disposed) return;
      attempt += 1;
      const wait = Math.min(2000, 200 * 2 ** Math.min(attempt, 4));
      retryTimer = setTimeout(connect, wait);
    };
  };

  connect();

  return {
    send,
    pause() {
      if (!paused) {
        paused = true;
        sendRaw(new Uint8Array([OP_PAUSE]));
      }
    },
    resume() {
      if (paused) {
        paused = false;
        sendRaw(new Uint8Array([OP_RESUME]));
      }
    },
    dispose() {
      disposed = true;
      if (retryTimer) clearTimeout(retryTimer);
      retryTimer = null;
      const current = ws;
      ws = null;
      current?.close();
    },
  };
}
