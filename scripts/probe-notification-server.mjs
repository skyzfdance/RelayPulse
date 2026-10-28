// 本地通知联调：每次正式探测交替成功／失败，模型同步不会推进顺序。
import { createServer } from 'node:http';

const model = 'relaypulse-notification-test';
const port = 18081;
let probes = 0;

function json(response, status, body) {
  response.writeHead(status, { 'Content-Type': 'application/json' });
  response.end(JSON.stringify(body));
}

const server = createServer((request, response) => {
  const path = new URL(request.url, `http://127.0.0.1:${port}`).pathname;
  request.resume();
  if (request.method === 'GET' && path === '/v1/models') {
    json(response, 200, {
      data: [{ id: model, type: 'model', display_name: model }],
      has_more: false,
    });
    return;
  }
  if (request.method !== 'POST' || path !== '/v1/messages') {
    json(response, 404, { error: { message: 'Use /v1/models or /v1/messages' } });
    return;
  }
  probes += 1;
  const success = probes % 2 === 1;
  console.log(`第 ${probes} 次探测：${success ? '正常（故障后应触发恢复通知）' : 'HTTP 503（应触发故障通知）'}`);
  if (!success) {
    json(response, 503, {
      type: 'error',
      error: { type: 'overloaded_error', message: 'RelayPulse 本地通知测试：模拟故障' },
    });
    return;
  }
  // 返回完整 Anthropic SSE，首 Token 与结束事件均走真实探测解析流程。
  response.writeHead(200, { 'Content-Type': 'text/event-stream' });
  const events = [
    {
      type: 'message_start',
      message: {
        id: `notification-test-${probes}`,
        type: 'message', role: 'assistant', model, content: [], stop_reason: null,
        usage: { input_tokens: 1, output_tokens: 0 },
      },
    },
    { type: 'content_block_start', index: 0, content_block: { type: 'text', text: '' } },
    { type: 'content_block_delta', index: 0, delta: { type: 'text_delta', text: 'OK' } },
    { type: 'content_block_stop', index: 0 },
    { type: 'message_delta', delta: { stop_reason: 'end_turn' }, usage: { output_tokens: 1 } },
    { type: 'message_stop' },
  ];
  for (const event of events) {
    response.write(`event: ${event.type}\ndata: ${JSON.stringify(event)}\n\n`);
  }
  response.end();
});

server.on('error', (error) => {
  console.error(error.code === 'EADDRINUSE'
    ? `端口 ${port} 已被占用，请关闭之前的测试服务后重试。`
    : `本地测试服务启动失败：${error.message}`);
  process.exitCode = 1;
});
server.listen(port, '127.0.0.1', () => {
  console.log(`通知测试服务：http://127.0.0.1:${port}`);
  console.log('临时站点：Anthropic 协议；凭据填 local-test；同步并启用唯一模型。');
  console.log('连续失败转红、连续成功恢复均设为 1；使用正式“探测”操作。');
  console.log('顺序：正常 → 故障通知 → 恢复通知 → 故障通知；Ctrl+C 停止。');
});
