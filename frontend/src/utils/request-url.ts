/** 请求地址只连接原始字符串；校验失败时拒绝，不自动补全或改写。 */
export function requestUrl(baseUrl: string, endpoint: string, sameOrigin = false): string {
  if (!baseUrl || baseUrl !== baseUrl.trim() || /[\r\n\t]/u.test(baseUrl))
    throw new Error('请输入有效的 Base URL');
  if (!endpoint.trim()) throw new Error('请输入端点');
  if (endpoint !== endpoint.trim() || /[\r\n\t]/u.test(endpoint))
    throw new Error('端点首尾不能有空格或包含换行');
  const base = new URL(baseUrl);
  const address = baseUrl + endpoint;
  const url = new URL(address);
  for (const target of [base, url]) {
    if (!['http:', 'https:'].includes(target.protocol) || target.username || target.password)
      throw new Error('请输入不含凭据的 HTTP 或 HTTPS 地址');
  }
  if (url.href !== address)
    throw new Error('拼接地址需要 URL 规范化，请直接修正配置；程序不会自动补全或纠正。');
  if (sameOrigin && url.origin !== base.origin) throw new Error('端点必须与站点 Base URL 同源');
  return address;
}
