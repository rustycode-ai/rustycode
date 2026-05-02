import { ModelSelector } from "./ModelSelector";
import type { ConnectionStatus } from "../hooks/useWebSocket";

export type { ConnectionStatus };

interface StatusBarProps {
  toolIterationCount: number;
  pending: boolean;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheCreationTokens: number;
  onToggleSidebar: () => void;
  onOpenSettings: () => void;
  onModelSwitch: (provider: string, model: string) => void;
  provider: string;
  model: string;
  connectionStatus: ConnectionStatus;
  modelSelectorTrigger?: number;
}

function formatTokens(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return `${(n / 1000).toFixed(1)}k`;
  return `${(n / 1_000_000).toFixed(1)}M`;
}

export function StatusBar({ toolIterationCount, pending, inputTokens, outputTokens, cacheReadTokens, cacheCreationTokens, onToggleSidebar, onOpenSettings, onModelSwitch, provider, model, connectionStatus, modelSelectorTrigger }: StatusBarProps) {
  return (
    <header className="status-bar" role="banner">
      <button
        className="btn-icon status-menu-btn"
        onClick={onToggleSidebar}
        aria-label="Toggle session sidebar"
        title="Sessions"
      >
        =
      </button>
      <span className="status-title">
        RustyCode
        <span className={`status-dot status-dot-${connectionStatus}`} title={connectionStatus} />
      </span>
      <span className="status-sep" />
      <ModelSelector provider={provider} model={model} onSwitch={onModelSwitch} triggerOpen={modelSelectorTrigger} />
      {pending && (
        <span className="status-pending" role="status" aria-live="polite">
          <span className="status-spinner" aria-hidden="true" />
          Generating
        </span>
      )}
      {toolIterationCount > 0 && (
        <span className="status-tools">Tools: {toolIterationCount}</span>
      )}
      {(inputTokens > 0 || outputTokens > 0) && (
        <span className="status-tokens">
          {formatTokens(inputTokens + outputTokens)} tokens
        </span>
      )}
      {cacheReadTokens > 0 && (
        <span className="status-cache" title={`Cache: ${cacheReadTokens} read, ${cacheCreationTokens} created`}>
          ∿ {formatTokens(cacheReadTokens)} cached
        </span>
      )}
      <span className="status-sep" />
      <button
        className="btn-icon status-settings-btn"
        onClick={onOpenSettings}
        aria-label="Settings"
        title="Settings"
      >
        :
      </button>
    </header>
  );
}
