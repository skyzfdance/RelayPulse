/** HTTP 字段语法；不决定凭据、协议默认值或业务上是否允许某个字段。 */
export function httpHeaderNameError(value: unknown): string | null {
  return typeof value === 'string' && /^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/u.test(value)
    ? null
    : '请求头名称格式无效';
}

export function httpHeaderValueError(value: unknown): string | null {
  if (
    typeof value !== 'string' ||
    value !== value.trim() ||
    [...value].some((char) => {
      const code = char.charCodeAt(0);
      return (code < 32 && code !== 9) || code === 127;
    })
  )
    return '请求头值格式无效，不能包含首尾空格或控制字符';
  return null;
}
