import type {
  Envelope,
  HelloPayload,
  InputPayload,
  HeartbeatPayload,
  SessionCreatedPayload,
  SessionResumedPayload,
  EventPayload,
  HeartbeatAckPayload,
  ErrorPayload,
  FrontendSession,
} from "./types";

export type ErrorClass =
  | "auth"
  | "rate_limit"
  | "server"
  | "session"
  | "connection";

export interface ClassifiedError {
  class: ErrorClass;
  code: string;
  message: string;
  retryable: boolean;
}

const RETRYABLE_CLASSES: Record<ErrorClass, boolean> = {
  auth: false,
  rate_limit: true,
  server: true,
  session: false,
  connection: true,
};

export function classifyError(code: string, message: string): ClassifiedError {
  const cls: ErrorClass =
    code === "unauthorized" || code === "session_expired"
      ? "auth"
      : code === "rate_limited"
        ? "rate_limit"
        : code === "session_not_found"
          ? "session"
          : "server";
  return { class: cls, code, message, retryable: RETRYABLE_CLASSES[cls] };
}

export function classifyWsClose(code: number, reason: string): ClassifiedError {
  const cls: ErrorClass =
    code === 4001 || code === 4003
      ? "auth"
      : code === 4008 || code === 429
        ? "rate_limit"
        : code >= 4500
          ? "server"
          : "connection";
  return {
    class: cls,
    code: `ws_close_${code}`,
    message: reason || `WebSocket closed with code ${code}`,
    retryable: RETRYABLE_CLASSES[cls],
  };
}

type ServerMessageHandler = (
  type: string,
  payload: unknown
) => void;

type PendingEnvelope = {
  type: string;
  id: string;
  payload: unknown;
};

export class WsClient {
  private ws: WebSocket | null = null;
  private sessionToken: string | null = null;
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null;
  private heartbeatIntervalMs = 30_000;
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 10;
  private baseReconnectDelay = 1000;
  private ready = false;
  private manualDisconnect = false;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private pendingSendQueue: PendingEnvelope[] = [];
  private static readonly MAX_PENDING_QUEUE = 64;
  private onMessage: ServerMessageHandler;
  private url: string;

  constructor(url: string, onMessage: ServerMessageHandler) {
    this.url = url;
    this.onMessage = onMessage;
  }

  connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      this.manualDisconnect = false;
      this.ws = new WebSocket(this.url);
      let handshakeResolved = false;

      this.ws.onopen = () => {
        this.ready = false;
        this.sendHello();
        this.startHeartbeat();
      };

      this.ws.onmessage = (event) => {
        const handled = this.handleMessage(event.data as string);
        if (!handshakeResolved && handled === "handshake") {
          handshakeResolved = true;
          this.reconnectAttempts = 0;
          this.ready = true;
          this.flushPendingSends();
          resolve();
        }
      };

      this.ws.onerror = () => {
        if (!handshakeResolved) {
          reject(new Error("WebSocket connection error"));
        }
      };

      this.ws.onclose = () => {
        this.ready = false;
        this.stopHeartbeat();
        if (this.manualDisconnect) {
          return;
        }
        if (!handshakeResolved) {
          reject(new Error("WebSocket connection closed before handshake"));
        }
        this.scheduleReconnect();
      };
    });
  }

  private sendEnvelope(type: string, id: string, payload: unknown): void {
    const envelope: Envelope = { v: 2, type, id, payload };
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      if (type !== "hello" && this.pendingSendQueue.length < WsClient.MAX_PENDING_QUEUE) {
        this.pendingSendQueue.push(envelope);
      }
      return;
    }
    if (type !== "hello" && !this.ready) {
      this.pendingSendQueue.push(envelope);
      return;
    }
    this.ws.send(JSON.stringify(envelope));
  }

  private sendHello(): void {
    const payload: HelloPayload = {};
    if (this.sessionToken) {
      payload.session_token = this.sessionToken;
    }
    this.sendEnvelope("hello", "1", payload);
  }

  sendInput(content: string): void {
    const payload: InputPayload = { content };
    this.sendEnvelope("input", crypto.randomUUID(), payload);
  }

  sendAbort(): void {
    this.sendEnvelope("abort", crypto.randomUUID(), {});
  }

  sendToolApproval(requestId: string, approved: boolean): void {
    this.sendEnvelope("tool_approval", crypto.randomUUID(), {
      request_id: requestId,
      approved,
    });
  }

  sendPlanApproval(planId: string, approved: boolean): void {
    this.sendEnvelope("plan_approval", crypto.randomUUID(), {
      plan_id: planId,
      approved,
    });
  }

  private sendHeartbeat(): void {
    const payload: HeartbeatPayload = { ts: Date.now() };
    this.sendEnvelope("heartbeat", crypto.randomUUID(), payload);
  }

  private startHeartbeat(): void {
    this.stopHeartbeat();
    this.heartbeatTimer = setInterval(() => this.sendHeartbeat(), this.heartbeatIntervalMs);
  }

  private stopHeartbeat(): void {
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
  }

  private handleMessage(raw: string): "handshake" | "regular" {
    let envelope: Envelope;
    try {
      envelope = JSON.parse(raw) as Envelope;
    } catch {
      console.error("Failed to parse envelope:", raw);
      return "regular";
    }

    switch (envelope.type) {
      case "session_created": {
        const payload = envelope.payload as SessionCreatedPayload;
        this.sessionToken = payload.session_token;
        if (payload.capabilities?.heartbeat_interval_ms) {
          this.heartbeatIntervalMs = payload.capabilities.heartbeat_interval_ms;
        }
        this.onMessage("session_created", payload);
        return "handshake";
      }
      case "session_resumed": {
        const payload = envelope.payload as SessionResumedPayload;
        this.sessionToken = payload.session_token;
        this.onMessage("session_resumed", payload);
        return "handshake";
      }
      case "state_snapshot": {
        const payload = envelope.payload as FrontendSession;
        this.onMessage("state_snapshot", payload);
        return "regular";
      }
      case "event": {
        const payload = envelope.payload as EventPayload;
        this.onMessage("event", payload);
        return "regular";
      }
      case "heartbeat_ack": {
        const payload = envelope.payload as HeartbeatAckPayload;
        this.onMessage("heartbeat_ack", payload);
        return "regular";
      }
      case "error": {
        const payload = envelope.payload as ErrorPayload;
        this.onMessage("error", payload);
        return "regular";
      }
      default:
        console.warn("Unknown message type:", envelope.type);
        return "regular";
    }
  }

  private flushPendingSends(): void {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN || !this.ready) {
      return;
    }

    while (this.pendingSendQueue.length > 0) {
      const envelope = this.pendingSendQueue.shift();
      if (envelope) {
        this.ws.send(JSON.stringify(envelope));
      }
    }
  }

  private scheduleReconnect(): void {
    if (this.reconnectAttempts >= this.maxReconnectAttempts) {
      this.onMessage("connection_lost", {
        message: "Max reconnection attempts reached",
      });
      return;
    }
    const baseDelay =
      this.baseReconnectDelay *
      Math.pow(2, this.reconnectAttempts);
    // Add jitter (±25%) to avoid thundering herd on simultaneous reconnects
    const jitter = baseDelay * 0.25 * (Math.random() * 2 - 1);
    const delay = Math.max(this.baseReconnectDelay, baseDelay + jitter);
    this.reconnectAttempts++;
    this.onMessage("reconnecting", { attempt: this.reconnectAttempts, delay });
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      this.connect().catch(() => {
        // onclose will schedule next reconnect
      });
    }, delay);
  }

  disconnect(): void {
    this.stopHeartbeat();
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.manualDisconnect = true;
    this.reconnectAttempts = this.maxReconnectAttempts; // prevent reconnect
    this.ready = false;
    this.pendingSendQueue = [];
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
  }

  get connected(): boolean {
    return this.ws?.readyState === WebSocket.OPEN;
  }

  get token(): string | null {
    return this.sessionToken;
  }
}
