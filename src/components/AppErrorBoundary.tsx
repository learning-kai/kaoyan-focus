import { RefreshCcw, TriangleAlert } from 'lucide-react';
import { Component, type ErrorInfo, type ReactNode } from 'react';

type AppErrorBoundaryProps = {
  children: ReactNode;
};

type AppErrorBoundaryState = {
  error: Error | null;
};

export default class AppErrorBoundary extends Component<AppErrorBoundaryProps, AppErrorBoundaryState> {
  state: AppErrorBoundaryState = {
    error: null,
  };

  static getDerivedStateFromError(error: Error): AppErrorBoundaryState {
    return { error };
  }

  override componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error('App crashed', error, errorInfo);
  }

  handleRetry = () => {
    this.setState({ error: null });
    window.location.reload();
  };

  override render() {
    if (this.state.error) {
      return (
        <div className="app-error-screen" role="alert">
          <div className="app-error-card">
            <span className="app-error-icon" aria-hidden="true">
              <TriangleAlert size={22} />
            </span>
            <div className="app-error-copy">
              <strong>应用加载失败</strong>
              <p>{this.state.error.message || '出现了未处理的运行时错误。'}</p>
            </div>
            <button className="primary-action" type="button" onClick={this.handleRetry}>
              <RefreshCcw size={16} />
              重新加载
            </button>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}
