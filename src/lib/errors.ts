/** 将任意异常转为可展示的短文案：去掉开头的 "Error: " 前缀，超长截断 */
export function errMsg(e: unknown): string {
  let text = String(e);
  const prefix = "Error: ";
  if (text.startsWith(prefix)) text = text.slice(prefix.length);
  if (text.length > 300) text = `${text.slice(0, 300)}…`;
  return text;
}
