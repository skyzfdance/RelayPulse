import { App as AntdApp, ConfigProvider } from 'antd';
import zhCN from 'antd/locale/zh_CN';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import { MockOperationHistoryDataSource } from './mock-operation-history-data-source';
import { OperationHistoryPage } from './OperationHistoryPage';
import type { OperationHistoryDataSource, OperationHistoryQuery } from './operation-history-types';

function renderPage(dataSource: OperationHistoryDataSource) {
  return render(
    <ConfigProvider locale={zhCN}>
      <AntdApp>
        <div id="relaypulse-overlay-root">
          <OperationHistoryPage dataSource={dataSource} />
        </div>
      </AntdApp>
    </ConfigProvider>,
  );
}

afterEach(cleanup);

describe('OperationHistoryPage', () => {
  it('renders the fixed four-column table without secondary operation text', async () => {
    renderPage(new MockOperationHistoryDataSource());

    expect(await screen.findByText('调整站点配置')).toBeVisible();
    const table = screen.getAllByRole('table')[0]!;
    expect(
      within(table)
        .getAllByRole('columnheader')
        .map((header) => header.textContent),
    ).toEqual(['时间', '类型', '操作', '详情']);
    const operationCell = screen.getByText('调整站点配置').closest('td');
    expect(operationCell).toHaveTextContent('调整站点配置');
    expect(operationCell?.querySelector('small')).toBeNull();
    expect(screen.queryByRole('columnheader', { name: /状态|结果|耗时|操作 ID/ })).toBeNull();
    expect(document.body.innerHTML).not.toContain('record-config-site-1');
  });

  it('sends typed category and safe operation text filters', async () => {
    const user = userEvent.setup();
    const mock = new MockOperationHistoryDataSource();
    const queries: OperationHistoryQuery[] = [];
    const dataSource: OperationHistoryDataSource = {
      async listRecords(query) {
        queries.push(query);
        return mock.listRecords(query);
      },
      getRecordDetails: (recordRef) => mock.getRecordDetails(recordRef),
    };
    renderPage(dataSource);

    expect(await screen.findByText('调整站点配置')).toBeVisible();
    await user.click(screen.getByLabelText('操作类型筛选'));
    await user.click(screen.getByText('配置变更', { selector: '.ant-select-item-option-content' }));
    await waitFor(() => expect(queries[queries.length - 1]?.category).toBe('configuration_change'));

    await user.type(screen.getByRole('textbox', { name: '搜索操作记录' }), '站点');
    await waitFor(() => expect(queries[queries.length - 1]?.operationQuery).toBe('站点'), {
      timeout: 1_500,
    });
    expect(queries[queries.length - 1]?.cursor).toBeUndefined();
    expect(queries[queries.length - 1]).not.toHaveProperty('status');
    expect(queries[queries.length - 1]).not.toHaveProperty('operationId');
  });

  it('loads the next cursor page without showing a total or page numbers', async () => {
    const user = userEvent.setup();
    renderPage(new MockOperationHistoryDataSource());

    expect(await screen.findByText('调整站点配置')).toBeVisible();
    expect(screen.queryByText('停用站点')).toBeNull();
    expect(screen.queryByText(/共 \d+ 条/)).toBeNull();
    await user.click(screen.getByRole('button', { name: '加载更多' }));
    expect(await screen.findByText('停用站点')).toBeVisible();
    expect(screen.getByText('已加载当前查询的全部记录')).toBeVisible();
  });

  it('opens details only from the required icon button and renders safe config changes', async () => {
    const user = userEvent.setup();
    renderPage(new MockOperationHistoryDataSource());

    const operation = await screen.findByText('调整站点配置');
    fireEvent.click(operation.closest('tr')!);
    expect(screen.queryByRole('dialog')).toBeNull();

    const detailButton = screen.getByRole('button', {
      name: '查看 调整站点配置 操作详情',
    });
    expect(detailButton).toHaveClass('ant-btn-text', 'ant-btn-circle');
    await user.click(detailButton);

    const drawer = await screen.findByRole('dialog');
    expect(
      await within(drawer).findByText('用户修改了 OpenAI 主线路的站点配置，保存成功。'),
    ).toBeVisible();
    expect(within(drawer).getByText('变更内容')).toBeVisible();
    expect(within(drawer).getByText('API Key')).toBeVisible();
    expect(within(drawer).getByText('已更新')).toBeVisible();
    expect(drawer).not.toHaveTextContent('operationId');
    expect(drawer).not.toHaveTextContent('sessionId');
  });

  it('distinguishes an empty dataset from an initial query failure', async () => {
    const { unmount } = renderPage(new MockOperationHistoryDataSource('empty'));
    expect(await screen.findByText('暂无操作记录')).toBeVisible();
    expect(screen.queryByText('操作记录加载失败')).toBeNull();
    unmount();

    renderPage(new MockOperationHistoryDataSource('error'));
    expect(await screen.findByText('操作记录加载失败')).toBeVisible();
    expect(screen.getByRole('button', { name: /重\s*试/ })).toBeEnabled();
    expect(screen.queryByText('暂无操作记录')).toBeNull();
  });

  it('recovers from an expired cursor by rebuilding the first page', async () => {
    const user = userEvent.setup();
    renderPage(new MockOperationHistoryDataSource('cursor-expired'));

    expect(await screen.findByText('调整站点配置')).toBeVisible();
    await user.click(screen.getByRole('button', { name: '加载更多' }));
    expect(await screen.findByText('日志内容已变化，已重新载入最新记录。')).toBeVisible();
    expect(screen.getByText('调整站点配置')).toBeVisible();
    expect(screen.queryByText('停用站点')).toBeNull();
  });

  it('does not append records from a different dataset version', async () => {
    const user = userEvent.setup();
    const mock = new MockOperationHistoryDataSource();
    let firstPageCalls = 0;
    const dataSource: OperationHistoryDataSource = {
      async listRecords(query) {
        const page = await mock.listRecords(query);
        if (query.cursor) return { ...page, datasetVersion: 'changed-dataset' };
        firstPageCalls += 1;
        return {
          ...page,
          datasetVersion: firstPageCalls === 1 ? 'initial-dataset' : 'changed-dataset',
        };
      },
      getRecordDetails: (recordRef) => mock.getRecordDetails(recordRef),
    };
    renderPage(dataSource);

    expect(await screen.findByText('调整站点配置')).toBeVisible();
    await user.click(screen.getByRole('button', { name: '加载更多' }));
    expect(await screen.findByText('日志内容已变化，已重新载入最新记录。')).toBeVisible();
    expect(screen.queryByText('停用站点')).toBeNull();
  });
});
