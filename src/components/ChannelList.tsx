import { channelProvider } from "../lib/models";
import type { ChannelBalance, ChannelStatus, QuotaTier } from "../types";
import { ProviderIcon } from "./ProviderIcons";

interface Props {
  channels: ChannelStatus[];
}

const WINDOW_LABELS: Record<string, string> = {
  "5h": "5小时",
  "7d": "7天",
  "7d-sonnet": "7天 Sonnet",
  "7d-fable": "7天 Fable",
  weekly: "每周",
  daily: "每日",
  "30d": "30天",
  total: "总计",
};

const TIER_LABELS: Record<string, string> = {
  requests: "请求",
  tokens: "Token",
  shared: "共享",
  pro: "Pro",
  flash: "Flash",
};

function statusDot(channel: ChannelStatus) {
  if (channel.status === "degraded") return "bg-amber-500";
  if (channel.online || channel.status === "operational") return "bg-emerald-500";
  return "bg-red-500";
}

function quotaColor(pct: number) {
  if (pct >= 90) return { bar: "bg-red-500", text: "text-red-600" };
  if (pct >= 75) return { bar: "bg-amber-500", text: "text-amber-600" };
  return { bar: "bg-emerald-500", text: "text-emerald-600" };
}

function tierLabel(tier: QuotaTier) {
  const window = WINDOW_LABELS[tier.window] ?? tier.window;
  if (!tier.label) return window;
  const extra = TIER_LABELS[tier.label] ?? tier.label;
  return `${extra}/${window}`;
}

function formatReset(iso: string) {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  const diff = date.getTime() - Date.now();
  if (diff <= 0) return "即将重置";
  if (diff < 3_600_000) return `${Math.max(1, Math.round(diff / 60_000))}m`;
  const hours = Math.round(diff / 3_600_000);
  if (hours < 48) return `${hours}h`;
  const mm = String(date.getMonth() + 1).padStart(2, "0");
  const dd = String(date.getDate()).padStart(2, "0");
  return `${mm}-${dd}`;
}

function formatBalance(item: ChannelBalance) {
  const amount = Number.isFinite(item.balance) ? item.balance.toFixed(2) : "-";
  const currency = item.currency === "$" ? "USD" : item.currency;
  return `${amount} ${currency}`;
}

function availColor(pct: number) {
  if (pct >= 95) return { bar: "bg-emerald-500", text: "text-emerald-600" };
  if (pct >= 80) return { bar: "bg-amber-500", text: "text-amber-600" };
  return { bar: "bg-red-500", text: "text-red-600" };
}

function AvailabilityBar({ value }: { value: number }) {
  const pct = Math.min(100, Math.max(0, value <= 1 ? value * 100 : value));
  const color = availColor(pct);
  return (
    <div className="mt-1.5 flex items-center gap-1.5 text-[11px]">
      <span className="w-16 shrink-0 truncate text-gray-500">成功率</span>
      <div className="h-1.5 min-w-0 flex-1 overflow-hidden rounded-full bg-gray-200">
        <div
          className={`h-full rounded-full transition-all ${color.bar}`}
          style={{ width: `${pct}%` }}
        />
      </div>
      <span className={`w-8 shrink-0 text-right font-medium ${color.text}`}>
        {Math.round(pct)}%
      </span>
    </div>
  );
}

function QuotaBars({ tiers }: { tiers: QuotaTier[] }) {
  if (tiers.length === 0) return null;
  return (
    <div className="mt-1.5 space-y-1">
      {tiers.map((tier, index) => {
        const pct = Math.min(100, Math.max(0, tier.used_percent));
        const color = quotaColor(pct);
        return (
          <div
            key={`${tier.window}-${tier.label ?? ""}-${index}`}
            className="flex items-center gap-1.5 text-[11px]"
          >
            <span className="w-16 shrink-0 truncate text-gray-500" title={tierLabel(tier)}>
              {tierLabel(tier)}
            </span>
            <div className="h-1.5 min-w-0 flex-1 overflow-hidden rounded-full bg-gray-200">
              <div
                className={`h-full rounded-full transition-all ${color.bar}`}
                style={{ width: `${pct}%` }}
              />
            </div>
            <span className={`w-8 shrink-0 text-right font-medium ${color.text}`}>
              {Math.round(pct)}%
            </span>
            {tier.reset_at && (
              <span className="w-10 shrink-0 truncate text-gray-400" title={tier.reset_at}>
                {formatReset(tier.reset_at)}
              </span>
            )}
          </div>
        );
      })}
    </div>
  );
}

export default function ChannelList({ channels }: Props) {
  if (channels.length === 0) {
    return <p className="text-sm text-gray-400">未解析到渠道数据</p>;
  }

  return (
    <ul className="max-h-72 space-y-1.5 overflow-auto pr-1">
      {channels.map((channel, index) => {
        const provider = channelProvider(channel);
        const balances = channel.balances ?? [];
        return (
        <li
          key={`${channel.name}-${index}`}
          className="rounded border border-gray-100 bg-gray-50 px-2 py-1.5 text-sm text-gray-700"
        >
          <div className="flex items-center gap-2">
            <i className={`h-2 w-2 shrink-0 rounded-full ${statusDot(channel)}`} />
            {provider && <ProviderIcon provider={provider} size={14} />}
            <span className="min-w-0 truncate font-medium">{channel.name}</span>
            {channel.plan_level && (
              <span className="shrink-0 rounded bg-gray-200 px-1.5 py-0.5 text-[10px] text-gray-600">
                {channel.plan_level}
              </span>
            )}
          </div>
          {channel.detail && (
            <p className="mt-0.5 pl-4 text-xs leading-5 text-gray-400">{channel.detail}</p>
          )}
          {channel.availability != null && (
            <AvailabilityBar value={channel.availability} />
          )}
          <QuotaBars tiers={channel.tiers ?? []} />
          {balances.length > 0 && (
            <div className="mt-1 flex flex-wrap gap-x-2 text-[11px]">
              {balances.map((item) => (
                <span
                  key={`${item.currency}-${item.balance}`}
                  className={item.balance <= 0 ? "font-medium text-red-600" : "text-gray-600"}
                >
                  余额 {formatBalance(item)}
                </span>
              ))}
            </div>
          )}
        </li>
        );
      })}
    </ul>
  );
}
