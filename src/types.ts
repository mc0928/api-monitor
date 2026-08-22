export type SiteType = "new2api" | "sub2api";

export interface SiteConfig {
  id: string;
  name: string;
  type: SiteType;
  base_url: string;
  vpn: boolean;
  /** new2api：个人访问令牌 */
  token?: string | null;
  /** new2api：New-Api-User 请求头；不填则后端不发该头 */
  user_id?: string | null;
  /** sub2api：登录账号（邮箱） */
  username?: string | null;
  /** sub2api：登录密码 */
  password?: string | null;
}

export type ProviderId = "gpt" | "claude" | "grok" | "kimi" | "gemini";

export interface MonitorModels {
  gpt: string[];
  claude: string[];
  grok: string[];
  kimi: string[];
  gemini: string[];
}

export interface MonitorConfig {
  models: MonitorModels;
}

export interface RefreshConfig {
  /** 自动刷新间隔（分钟）：0=关闭，可选 0/5/10/30，默认 5（读取处防御式 ?? 5） */
  interval_minutes: number;
}

export interface AppConfig {
  proxy: { url: string };
  sites: SiteConfig[];
  monitor: MonitorConfig;
  /** 自动刷新（缺省时按 interval_minutes=5 处理） */
  refresh?: RefreshConfig;
  /** 调试模式：false 时后端会剥离 raw 字段（读取处防御式 ?? false） */
  debug?: boolean;
  /** 卡片排序：auto 按成功率 / manual 按配置顺序（缺省 auto） */
  sort_by?: "auto" | "manual";
}

export interface QuotaTier {
  window: string;
  label: string | null;
  used_percent: number;
  used: number | null;
  limit: number | null;
  reset_at: string | null;
}

export interface ChannelBalance {
  currency: string;
  balance: number;
}

export interface ChannelStatus {
  name: string;
  online: boolean;
  detail: string;
  /** operational | degraded | failed | unknown */
  status: string;
  plan_level: string | null;
  /** gpt | claude | grok | kimi */
  provider?: string | null;
  model?: string | null;
  availability: number | null;
  latency_ms: number | null;
  tiers: QuotaTier[];
  balances: ChannelBalance[];
}

export interface SiteResult {
  id: string;
  name: string;
  site_type: SiteType;
  ok: boolean;
  /** 毫秒时间戳 */
  checked_at: number;
  error: string | null;
  /** 非致命的补充提示（如令牌无权限拉取渠道列表） */
  note: string | null;
  /** new2api：quota 原值（500000 = $1） */
  quota: number | null;
  /** 站点余额（美元）：new2api 账户余额，或 sub2api 渠道 USD 合计 */
  balance_usd: number | null;
  /** new2api：累计请求数 */
  request_count: number | null;
  /** 渠道状态列表 */
  channels: ChannelStatus[];
  /** 原始响应片段（调试用） */
  raw: string | null;
}
