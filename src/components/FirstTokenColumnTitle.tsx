import { QuestionCircleOutlined } from '@ant-design/icons';
import { Tooltip } from 'antd';

const firstTokenHelp =
  '流式响应按 sub2api 口径计算：忽略 response.created 和 response.in_progress，以首个后续有效 SSE 事件作为首 Token。该时间由桌面端端到端观测，包含网络和站点转发耗时。“—”表示本次非流式响应，或请求在首个有效事件前失败、超时。';

export function FirstTokenColumnTitle() {
  return (
    <span className="inline-flex items-center gap-1 whitespace-nowrap">
      <span>首 Token</span>
      <Tooltip title={firstTokenHelp}>
        <QuestionCircleOutlined
          className="cursor-help text-rp-tertiary"
          role="img"
          aria-label="首 Token 数据说明"
          tabIndex={0}
        />
      </Tooltip>
    </span>
  );
}
