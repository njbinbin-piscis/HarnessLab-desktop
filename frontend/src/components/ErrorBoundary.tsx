import { Component, ErrorInfo, ReactNode } from "react";
import { useT } from "../i18n/context";

interface Props {
  children: ReactNode;
}

interface State {
  error: Error | null;
  info: string;
}

function ErrorFallback({ error, info, onRetry }: { error: Error; info: string; onRetry: () => void }) {
  const t = useT();
  return (
    <div className="error-boundary">
      <h3>{t("errorBoundary.title")}</h3>
      <p className="eb-msg">{error.message}</p>
      {error.stack ? <pre className="eb-stack">{error.stack}</pre> : null}
      {info ? <pre className="eb-stack">{info}</pre> : null}
      <button type="button" className="btn btn-primary" onClick={onRetry}>{t("common.retry")}</button>
    </div>
  );
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null, info: "" };

  static getDerivedStateFromError(error: Error): Partial<State> {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    // eslint-disable-next-line no-console
    console.error("UI crashed:", error, info);
    this.setState({ info: info.componentStack ?? "" });
  }

  render(): ReactNode {
    const { error, info } = this.state;
    if (!error) return this.props.children;
    return (
      <ErrorFallback
        error={error}
        info={info}
        onRetry={() => this.setState({ error: null, info: "" })}
      />
    );
  }
}
