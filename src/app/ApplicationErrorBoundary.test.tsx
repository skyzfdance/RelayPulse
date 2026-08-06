import { render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApplicationErrorBoundary } from './ApplicationErrorBoundary';
import { reportFrontendError } from './frontend-diagnostics';

vi.mock('./frontend-diagnostics', () => ({
  reportFrontendError: vi.fn(() => 'fe_fixture'),
}));

function BrokenView() {
  throw new Error('secret render failure');
  return null;
}

describe('ApplicationErrorBoundary', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('reports React render crashes and shows a stable error id without the raw error', () => {
    vi.spyOn(console, 'error').mockImplementation(() => undefined);

    render(
      <ApplicationErrorBoundary>
        <BrokenView />
      </ApplicationErrorBoundary>,
    );

    expect(reportFrontendError).toHaveBeenCalledWith(
      'react-render',
      expect.any(Error),
      expect.any(String),
    );
    expect(screen.getByText('界面发生错误')).toBeInTheDocument();
    expect(screen.getByText('错误编号：fe_fixture')).toBeInTheDocument();
    expect(screen.queryByText(/secret render failure/)).not.toBeInTheDocument();
  });
});
