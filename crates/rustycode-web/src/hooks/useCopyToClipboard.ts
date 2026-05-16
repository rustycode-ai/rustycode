import { useState, useCallback, useRef, useEffect } from "react";

export function useCopyToClipboard(resetDelay = 2000) {
  const [copied, setCopied] = useState(false);
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const copy = useCallback((text: string) => {
    try {
      navigator.clipboard.writeText(text).then(
        () => {
          setCopied(true);
          if (timeoutRef.current) clearTimeout(timeoutRef.current);
          timeoutRef.current = setTimeout(() => setCopied(false), resetDelay);
        },
        () => setCopied(false),
      );
    } catch {
      setCopied(false);
    }
  }, [resetDelay]);

  useEffect(() => () => {
    if (timeoutRef.current) clearTimeout(timeoutRef.current);
  }, []);

  return { copied, copy };
}
