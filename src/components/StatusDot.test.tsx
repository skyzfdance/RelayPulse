import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { StatusDot } from './StatusDot';

describe('StatusDot', () => {
  it.each([
    ['healthy', '正常'],
    ['warning', '注意'],
    ['error', '异常'],
    ['muted', '未启用'],
  ] as const)('exposes %s as %s', (status, label) => {
    render(<StatusDot status={status} />);
    expect(screen.getByRole('img', { name: label })).toBeInTheDocument();
  });
});
