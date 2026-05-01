// Generated from rustycode-ui-model types

export interface FrontendMessage {
  content: string;
  kind: FrontendMessageKind;
}

export enum FrontendMessageKind {
  User = "User",
  Assistant = "Assistant",
  System = "System",
  Tool = "Tool",
  Error = "Error",
}

export interface FrontendSession {
  input: string;
  messages: FrontendMessage[];
  last_user_prompt: string | null;
  pending_request: boolean;
  tool_iteration_count: number;
  current_response: string;
}

export interface WebSocketMessage {
  v: number;
  type: string;
  payload: any;
}

export interface InputPayload {
  content: string;
}

export interface ErrorPayload {
  code: string;
  message: string;
}
