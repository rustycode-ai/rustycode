import { Component, type ErrorInfo, type ReactNode } from "react";

interface SectionErrorBoundaryProps {
  children: ReactNode;
  name: string;
}

interface SectionErrorBoundaryState {
  hasError: boolean;
  message: string;
}

export class SectionErrorBoundary extends Component<SectionErrorBoundaryProps, SectionErrorBoundaryState> {
  state: SectionErrorBoundaryState = { hasError: false, message: "" };

  static getDerivedStateFromError(error: Error): SectionErrorBoundaryState {
    return { hasError: true, message: error.message };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error(`[${this.props.name}] render error:`, error, info.componentStack);
  }

  render(): ReactNode {
    if (this.state.hasError) {
      return (
        <div className="section-error" role="alert">
          <p>{this.props.name} encountered an error: {this.state.message}</p>
          <button onClick={() => this.setState({ hasError: false, message: "" })} type="button" aria-label={`Retry ${this.props.name}`}>
            Retry
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
