import { useState, useEffect, useCallback, useRef } from "react";

interface ProviderInfo {
  provider: string;
  model: string;
}

interface ProviderEntry {
  name: string;
  display_name: string;
  models: string[];
  default_model: string;
  available: boolean;
}

interface ProviderListResponse {
  current: ProviderInfo;
  providers: ProviderEntry[];
}

interface ModelSelectorProps {
  model: string;
  provider: string;
  onSwitch?: (provider: string, model: string) => void;
}

function matchesSearch(query: string, provider: string, model: string): boolean {
  const q = query.toLowerCase();
  return (
    provider.toLowerCase().includes(q) ||
    model.toLowerCase().includes(q)
  );
}

export function ModelSelector({ model, provider, onSwitch }: ModelSelectorProps) {
  const [data, setData] = useState<ProviderListResponse | null>(null);
  const [open, setOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(false);
  const [search, setSearch] = useState("");
  const [highlightIdx, setHighlightIdx] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  const fetchData = useCallback(async () => {
    setLoading(true);
    setError(false);
    try {
      const res = await fetch("/api/providers");
      if (res.ok) {
        const json: ProviderListResponse = await res.json();
        setData(json);
      } else {
        setError(true);
      }
    } catch {
      setError(true);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchData();
  }, [fetchData]);

  const current = data?.current ?? { provider, model };

  const handleOpen = () => {
    fetchData();
    setSearch("");
    setHighlightIdx(0);
    setOpen(true);
  };

  const switchModel = async (prov: string, mdl: string) => {
    try {
      const res = await fetch("/api/providers/switch", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ provider: prov, model: mdl }),
      });
      if (res.ok) {
        const info: ProviderInfo = await res.json();
        setData((prev) =>
          prev ? { ...prev, current: info } : prev
        );
        onSwitch?.(prov, mdl);
      } else {
        setError(true);
      }
    } catch {
      setError(true);
    }
    setOpen(false);
  };

  // Build flat list of selectable items for keyboard nav
  const items: Array<{ provider: string; providerDisplay: string; model: string; available: boolean }> = [];
  const providers = data?.providers ?? [];
  for (const p of providers) {
    for (const m of p.models) {
      if (!search || matchesSearch(search, p.name, m)) {
        items.push({
          provider: p.name,
          providerDisplay: p.display_name,
          model: m,
          available: p.available,
        });
      }
    }
  }

  // Keyboard handling inside the modal
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      setOpen(false);
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      setHighlightIdx((i) => Math.min(i + 1, items.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setHighlightIdx((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter" && items[highlightIdx]) {
      e.preventDefault();
      const item = items[highlightIdx];
      if (item.available) {
        switchModel(item.provider, item.model);
      }
    }
  };

  // Scroll highlighted item into view
  useEffect(() => {
    if (open && listRef.current) {
      const highlighted = listRef.current.querySelector("[data-highlighted='true']");
      highlighted?.scrollIntoView({ block: "nearest" });
    }
  }, [open, highlightIdx]);

  const displayProvider = current.provider || "unknown";
  const displayModel =
    current.model.length > 28
      ? current.model.slice(0, 26) + "..."
      : current.model || "unknown";

  if (!open) {
    return (
      <button
        className="model-selector-btn"
        onClick={handleOpen}
        aria-label="Switch model"
        title={`${displayProvider} / ${current.model}`}
      >
        <span className="model-provider">{displayProvider}</span>
        <span className="model-sep">/</span>
        <span className="model-name">{displayModel}</span>
      </button>
    );
  }

  // Group items by provider for rendering
  const grouped: Record<string, { display: string; available: boolean; models: typeof items }> = {};
  for (const item of items) {
    if (!grouped[item.provider]) {
      grouped[item.provider] = {
        display: item.providerDisplay,
        available: item.available,
        models: [],
      };
    }
    grouped[item.provider]!.models.push(item);
  }

  return (
    <div className="model-modal-overlay" onClick={() => setOpen(false)} role="dialog" aria-label="Switch model" aria-modal="true">
      <div className="model-modal" onClick={(e) => e.stopPropagation()} onKeyDown={handleKeyDown}>
        <div className="model-modal-header">
          <input
            ref={inputRef}
            className="model-search"
            type="text"
            placeholder="Search models..."
            value={search}
            autoFocus
            onChange={(e) => {
              setSearch(e.target.value);
              setHighlightIdx(0);
            }}
            aria-label="Search models"
          />
        </div>
        <div className="model-modal-list" ref={listRef}>
          {loading ? (
            <div className="model-loading" aria-hidden="true">
              <div className="skeleton skeleton-text" />
              <div className="skeleton skeleton-text" />
              <div className="skeleton skeleton-text skeleton-text-sm" />
              <div className="skeleton skeleton-text" />
              <div className="skeleton skeleton-text skeleton-text-sm" />
            </div>
          ) : Object.entries(grouped).map(([provId, group]) => (
            <div key={provId} className="model-group">
              <div className="model-group-header">
                <span className={`model-avail-dot ${group.available ? "dot-on" : "dot-off"}`} />
                <span className="model-group-name">{group.display}</span>
                {!group.available && <span className="model-unavail-label">no key</span>}
              </div>
              {group.models.map((item) => {
                const flatIdx = items.indexOf(item);
                const isCurrent =
                  item.provider === current.provider &&
                  item.model === current.model;
                return (
                  <button
                    key={`${item.provider}-${item.model}`}
                    className={`model-item ${isCurrent ? "model-item-active" : ""} ${flatIdx === highlightIdx ? "model-item-highlight" : ""}`}
                    data-highlighted={flatIdx === highlightIdx}
                    disabled={!item.available}
                    aria-disabled={!item.available}
                    onClick={() => switchModel(item.provider, item.model)}
                    onMouseEnter={() => setHighlightIdx(flatIdx)}
                  >
                    <span className="model-item-name">{item.model}</span>
                    {isCurrent && <span className="model-item-check" aria-hidden="true">&#10003;</span>}
                  </button>
                );
              })}
            </div>
          ))}
          {items.length === 0 && !loading && (
            <div className="model-empty">No models match your search.</div>
          )}
          {error && (
            <div className="model-error">
              Failed to load providers
              <button className="sidebar-retry" onClick={fetchData} type="button">Retry</button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
