import { ReloadOutlined } from '@ant-design/icons';
import { Button, Result } from 'antd';
import { Component, type ErrorInfo, type ReactNode } from 'react';
import { reportFrontendError } from './frontend-diagnostics';

interface ApplicationErrorBoundaryState {
  failed: boolean;
  errorId: string | null;
}

export class ApplicationErrorBoundary extends Component<
  { children: ReactNode },
  ApplicationErrorBoundaryState
> {
  state: ApplicationErrorBoundaryState = { failed: false, errorId: null };

  static getDerivedStateFromError(): Partial<ApplicationErrorBoundaryState> {
    return { failed: true };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    const errorId = reportFrontendError('react-render', error, info.componentStack);
    this.setState({ errorId });
  }

  render() {
    if (!this.state.failed) return this.props.children;
    return (
      <main className="grid h-full w-full place-items-center bg-rp-canvas px-6">
        <Result
          status="error"
          title="界面发生错误"
          subTitle={
            this.state.errorId ? `错误编号：${this.state.errorId}` : '错误信息正在写入诊断日志。'
          }
          extra={
            <Button
              type="primary"
              icon={<ReloadOutlined />}
              onClick={() => window.location.reload()}
            >
              重新加载
            </Button>
          }
        />
      </main>
    );
  }
}
