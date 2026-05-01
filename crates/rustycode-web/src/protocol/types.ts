// Types matching rustycode-protocol::StreamEvent and rustycode-ui-model::FrontendSession
// Tagged union serialization: { type: "VariantName", data: { ...fields } }

export interface StreamEventTextDelta {
  type: "text_delta";
  data: { content: string };
}
export interface StreamEventThinkingDelta {
  type: "thinking_delta";
  data: { content: string };
}
export interface StreamEventToolCallStarted {
  type: "tool_call_started";
  data: { id: string; name: string };
}
export interface StreamEventToolInputDelta {
  type: "tool_input_delta";
  data: { id: string; chunk: string };
}
export interface StreamEventToolExecStarted {
  type: "tool_exec_started";
  data: { id: string; name: string };
}
export interface StreamEventToolExecCompleted {
  type: "tool_exec_completed";
  data: { id: string; name: string; output: string; is_error: boolean };
}
export interface StreamEventTurnStarted {
  type: "turn_started";
  data: { turn: number };
}
export interface StreamEventTokenUsage {
  type: "token_usage";
  data: { input_tokens: number; output_tokens: number };
}
export interface StreamEventTurnCompleted {
  type: "turn_completed";
  data: { stop_reason: string };
}
export interface StreamEventCacheUsage {
  type: "cache_usage";
  data: { cache_read_tokens: number; cache_creation_tokens: number };
}
export interface StreamEventDone {
  type: "done";
  data: Record<string, never>;
}

export type StreamEvent =
  | StreamEventTextDelta
  | StreamEventThinkingDelta
  | StreamEventToolCallStarted
  | StreamEventToolInputDelta
  | StreamEventToolExecStarted
  | StreamEventToolExecCompleted
  | StreamEventTurnStarted
  | StreamEventTokenUsage
  | StreamEventTurnCompleted
  | StreamEventCacheUsage
  | StreamEventDone;

// Message Parts — rich content within a single message

export interface TextPart {
  type: "text";
  content: string;
}

export interface ThinkingPart {
  type: "thinking";
  content: string;
}

export interface ToolCallPart {
  type: "tool_call";
  id: string;
  name: string;
  status: "pending" | "running" | "completed" | "error";
  input?: string;
  output?: string;
  startedAt?: number;
  completedAt?: number;
}

export interface ErrorPart {
  type: "error";
  message: string;
}

export type MessagePart = TextPart | ThinkingPart | ToolCallPart | ErrorPart;

// Frontend model types matching rustycode-ui-model

export type FrontendMessageKind =
  | "User"
  | "Assistant"
  | "System"
  | "Tool"
  | "Error";

export interface FrontendMessage {
  id: string;
  content: string;
  kind: FrontendMessageKind;
  parts: MessagePart[];
}

export interface FrontendSession {
  input: string;
  messages: FrontendMessage[];
  last_user_prompt: string | null;
  pending_request: boolean;
  tool_iteration_count: number;
  current_response: string;
  input_tokens: number;
  output_tokens: number;
}

// Protocol v2 envelope

export interface Envelope {
  v: number;
  type: string;
  id: string;
  payload: unknown;
}

// Client → Server payloads

export interface HelloPayload {
  session_token?: string;
}

export interface InputPayload {
  content: string;
}

export interface HeartbeatPayload {
  ts: number;
}

// Server → Client payloads

export interface SessionCreatedPayload {
  session_token: string;
  capabilities: { streaming: boolean; heartbeat_interval_ms: number };
}

export interface SessionResumedPayload {
  session_token: string;
}

export interface EventPayload {
  seq: number;
  event: StreamEvent;
}

export interface HeartbeatAckPayload {
  ts: number;
  server_ts: number;
}

export interface ErrorPayload {
  code: string;
  message: string;
}
