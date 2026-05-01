import { useState, useEffect, useCallback } from "react";

interface ProviderInfo {
  provider: string;
  model: string;
}

interface ModelSelectorProps {
  model: string;
  provider: string;
}

function shortenModel(name: string): string {
  if (name.length > 28) return name.slice(0, 26) + "...";
  return name;
}

export function ModelSelector({ model, provider }: ModelSelectorProps) {
  const [info, setInfo] = useState<ProviderInfo>({ provider, model });
  const [showTooltip, setShowTooltip] = useState(false);

  const fetchInfo = useCallback(async () => {
    try {
      const res = await fetch("/api/providers");
      if (res.ok) {
        const data: ProviderInfo = await res.json();
        setInfo(data);
      }
    } catch {
      // Server not available
    }
  }, []);

  useEffect(() => {
    if (!model) fetchInfo();
  }, [model, fetchInfo]);

  const displayProvider = info.provider || "unknown";
  const displayModel = shortenModel(info.model || "unknown");

  return (
    <span
      className="model-selector"
      onMouseEnter={() => setShowTooltip(true)}
      onMouseLeave={() => setShowTooltip(false)}
    >
      <span className="model-provider">{displayProvider}</span>
      <span className="model-sep">/</span>
      <span className="model-name">{displayModel}</span>
      {showTooltip && (
        <span className="model-tooltip" role="tooltip">
          {info.provider} / {info.model}
        </span>
      )}
    </span>
  );
}
