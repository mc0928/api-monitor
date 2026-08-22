import { channelProvider } from "../lib/models";
import { useI18n } from "../lib/i18n";
import type { ChannelBalance, ChannelStatus, QuotaTier, TrendPoint } from "../types";
import { ProviderIcon } from "./ProviderIcons";

interface Props {
  channels: ChannelStatus[];
}

function statusDot(channel: ChannelStatus) {
  if (channel.status === "degraded") return "bg-amber-500";
  if (channel.online || channel.status === "operational") return "bg-emerald-500";
  return "bg-red-500";
}

function quotaColor(pct: number) {
  if (pct >= 90) return { bar: "bg-red-500", text: "text-red-600 dark:text-red-400" };
  if (pct >= 75) return { bar: "bg-amber-500", text: "text-amber-600 dark:text-amber-400" };
  return { bar: "bg-emerald-500", text: "text-emerald-600 dark:text-emerald-400" };
}

function availColor(pct: number) {
  if (pct >= 95) return { bar: "bg-emerald-500", text: "text-emerald-600 dark:text-emerald-400" };
  if (pct >= 80) return { bar: "bg-amber-500", text: "text-amber-600 dark:text-amber-400" };
  return { bar: "bg-red-500", text: "text-red-600 dark:text-red-400" };
}

function formatReset(iso: string) {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  const diff = date.getTime() - Date.now();
  if (diff <= 0) return "";
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

/** 成功率迷你趋势线：逐时数据（近 24h）；仅 1 个点时画平线（数据刚产生） */
function Sparkline({ points }: { points: TrendPoint[] }) {
  const vs = points.map((p) => Math.min(100, Math.max(0, p.v)));
  if (vs.length === 1) vs.push(vs[0]);
  const w = 92;
  const h = 24;
  const pad = 2;
  const min = Math.min(...vs);
  const max = Math.max(...vs);
  const x = (i: number) => pad + (i / (vs.length - 1)) * (w - 2 * pad);
  const y = (v: number) => h - pad - ((v - min) / (max - min || 1)) * (h - 2 * pad);
  const d = vs
    .map((v, i) => `${i ? "L" : "M"}${x(i).toFixed(1)},${y(v).toFixed(1)}`)
    .join(" ");
  const last = vs[vs.length - 1];
  const stroke = last >= 95 ? "#10b981" : last >= 80 ? "#f59e0b" : "#ef4444";
  const first = points[0];
  const lastP = points[points.length - 1];
  const hhmm = (iso: string) => (iso.length >= 16 ? iso.slice(11, 16) : iso);
  const title =
    points.length === 1
      ? `${hhmm(first.t)} · ${Math.round(first.v)}%`
      : `${hhmm(first.t)} → ${hhmm(lastP.t)} · ${Math.round(first.v)}% → ${Math.round(last)}%`;
  return (
    <svg width={w} height={h} viewBox={`0 0 ${w} ${h}`} className="shrink-0" aria-hidden>
      <title>{title}</title>
      <path
        d={d}
        fill="none"
        stroke={stroke}
        strokeWidth="1.5"
        strokeLinejoin="round"
        strokeLinecap="round"
      />
    </svg>
  );
}

function AvailabilityBar({ value, trend }: { value: number; trend?: TrendPoint[] | null }) {
  const { t } = useI18n();
  const pct = Math.min(100, Math.max(0, value <= 1 ? value * 100 : value));
  const color = availColor(pct);
  return (
    <div className="mt-1.5 flex items-center gap-1.5 text-[11px]">
      <span className="w-16 shrink-0 truncate text-gray-500 dark:text-gray-400">
        {t("list.successRate")}
      </span>
      <div className="h-1.5 min-w-0 flex-1 overflow-hidden rounded-full bg-gray-200 dark:bg-gray-700">
        <div
          className={`h-full rounded-full transition-all ${color.bar}`}
          style={{ width: `${pct}%` }}
        />
      </div>
      <span className={`w-8 shrink-0 text-right font-medium ${color.text}`}>
        {Math.round(pct)}%
      </span>
      {trend && trend.length > 0 && <Sparkline points={trend} />}
    </div>
  );
}

function QuotaBars({ tiers }: { tiers: QuotaTier[] }) {
  const { t } = useI18n();
  if (tiers.length === 0) return null;

  const tierLabel = (tier: QuotaTier) => {
    const window = t(`list.window.${tier.window}`);
    if (!tier.label) return window;
    const extra = t(`list.tier.${tier.label}`);
    return `${extra}/${window}`;
  };

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
            <span
              className="w-16 shrink-0 truncate text-gray-500 dark:text-gray-400"
              title={tierLabel(tier)}
            >
              {tierLabel(tier)}
            </span>
            <div className="h-1.5 min-w-0 flex-1 overflow-hidden rounded-full bg-gray-200 dark:bg-gray-700">
              <div
                className={`h-full rounded-full transition-all ${color.bar}`}
                style={{ width: `${pct}%` }}
              />
            </div>
            <span className={`w-8 shrink-0 text-right font-medium ${color.text}`}>
              {Math.round(pct)}%
            </span>
            {tier.reset_at && (
              <span
                className="w-10 shrink-0 truncate text-gray-400"
                title={`${t("list.resetSoon")} ${tier.reset_at}`}
              >
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
  const { t } = useI18n();
  if (channels.length === 0) {
    return <p className="text-sm text-gray-400">{t("list.noData")}</p>;
  }

  return (
    <ul className="max-h-72 space-y-1.5 overflow-auto pr-1">
      {channels.map((channel, index) => {
        const provider = channelProvider(channel);
        const balances = channel.balances ?? [];
        return (
          <li
            key={`${channel.name}-${channel.model ?? ""}-${index}`}
            className="rounded border border-gray-100 bg-gray-50 px-2 py-1.5 text-sm text-gray-700 dark:border-gray-800 dark:bg-gray-800/60 dark:text-gray-200"
          >
            <div className="flex items-center gap-2">
              <i className={`h-2 w-2 shrink-0 rounded-full ${statusDot(channel)}`} />
              {provider && <ProviderIcon provider={provider} size={14} />}
              <span className="min-w-0 truncate font-medium">{channel.name}</span>
              {channel.plan_level && (
                <span className="shrink-0 rounded bg-gray-200 px-1.5 py-0.5 text-[10px] text-gray-600 dark:bg-gray-700 dark:text-gray-300">
                  {channel.plan_level}
                </span>
              )}
            </div>
            {channel.detail && (
              <p className="mt-0.5 pl-4 text-xs leading-5 text-gray-400">{channel.detail}</p>
            )}
            {channel.availability != null && (
              <AvailabilityBar value={channel.availability} trend={channel.trend} />
            )}
            <QuotaBars tiers={channel.tiers ?? []} />
            {balances.length > 0 && (
              <div className="mt-1 flex flex-wrap gap-x-2 text-[11px]">
                {balances.map((item, index) => (
                  <span
                    key={`${item.currency}-${index}`}
                    className={
                      item.balance <= 0
                        ? "font-medium text-red-600 dark:text-red-400"
                        : "text-gray-600 dark:text-gray-400"
                    }
                  >
                    {t("list.balance")} {formatBalance(item)}
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
