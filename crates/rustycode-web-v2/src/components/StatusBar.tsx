interface StatusBarProps {
  toolIterationCount: number;
  pending: boolean;
}

export function StatusBar({ toolIterationCount, pending }: StatusBarProps) {
  return (
    <div className="status-bar">
      <span className="status-title">RustyCode</span>
      {pending && <span className="status-pending">Working...</span>}
      {toolIterationCount > 0 && (
        <span className="status-tools">Tools: {toolIterationCount}</span>
      )}
    </div>
  );
}
