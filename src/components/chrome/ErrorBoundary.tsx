import { Component } from "react";
import type { ErrorInfo, ReactNode } from "react";

import { Button } from "../ui/controls";

interface Props {
  children: ReactNode;
}

/** Keeps a bug in one internal page from taking the whole window down. */
export class ErrorBoundary extends Component<Props, { error: Error | null }> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("page crashed", error, info.componentStack);
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="page-crash" role="alert">
        <h2>This page hit a problem</h2>
        <p className="dim">{this.state.error.message}</p>
        <Button onClick={() => this.setState({ error: null })}>Try again</Button>
      </div>
    );
  }
}
