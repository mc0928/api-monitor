import { channelProvider } from "../lib/models";
import { useI18n } from "../lib/i18n";
import type { ChannelBalance, ChannelStatus, QuotaTier, TrendPoint } from "../types";
import { ProviderIcon } from "./ProviderIcons";

interface Props {
  channels: ChannelStatus[];
}

function statusVisual(channel: ChannelStatus, t: (key: string) => string) {
  if (channel.status === "degraded") {
    return {
      dot: "bg-amber-500",
      label: t("card.degraded"),
      badge: "bg-amber-100 text-amber-700 dark:bg-amber-950/60 dark:text-amber-300",
    };
  }
  if (channel.online || channel.status === "operational") {
    return {
      dot: "bg-emerald-500",
      label: t("card.normal"),
      badge: "bg-emerald-100 text-emerald-700 dark:bg-emerald-950/60 dark:text-emerald-300",
    };
  }
  if (channel.status === "failed") {
    return {
      dot: "bg-red-500",
      label: t("card.failed"),
      badge: "bg-red-100 text-red-700 dark:bg-red-950/60 dark:text-red-300",
    };
  }
  return {
    dot: "bg-gray-400",
    label: t("status.unchecked"),
    badge: "bg-gray-200 text-gray-600 dark:bg-gray-700 dark:text-gray-300",
  };
}

function formatRatio(value: number) {
  return Number.isInteger(value) ? value.toFixed(0) : value.toFixed(6).replace(/0+$/, "");
}

function quotaColor(pct: number) {
  if (pct >= 90) return { bar: "bg-red-500", text: "text-red-600 dark:text-red-400" };
  if (pct >= 75) return { bar: "bg-amber-500", text: "text-amber-600 dark:text-amber-400" };
  return { bar: "bg-emerald-500", text: "text-emerald-600 dark:text-emerald-400" };
}

function availColor(pct: number) {
  if (pct >= 95) {
    return {
      stroke: "#10b981",
      badge: "bg-emerald-100 text-emerald-700 dark:bg-emerald-950/60 dark:text-emerald-300",
    };
  }
  if (pct >= 80) {
    return {
      stroke: "#f59e0b",
      badge: "bg-amber-100 text-amber-700 dark:bg-amber-950/60 dark:text-amber-300",
    };
  }
  return {
    stroke: "#ef4444",
    badge: "bg-red-100 text-red-700 dark:bg-red-950/60 dark:text-red-300",
  };
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

/** 成功率趋势线：固定使用 0~100% 纵轴，每段按该点成功率显示绿/黄/红。 */
function Sparkline({ points }: { points: TrendPoint[] }) {
  const ordered = [...points]
    .filter((point) => Number.isFinite(point.v))
    .sort((a, b) => a.t.localeCompare(b.t));
  const unique = [...new Map(ordered.map((point) => [point.t, point])).values()];
  const plotted = unique.length === 1 ? [unique[0], { ...unique[0], t: `${unique[0].t} ` }] : unique;
  if (plotted.length === 0) return null;

  const w = 240;
  const h = 34;
  const pad = 3;
  const x = (i: number) => pad + (i / (plotted.length - 1)) * (w - 2 * pad);
  const y = (value: number) => h - pad - (Math.min(100, Math.max(0, value)) / 100) * (h - 2 * pad);
  const first = unique[0];
  const lastP = unique[unique.length - 1];
  const last = Math.min(100, Math.max(0, lastP.v));
  const hhmm = (iso: string) => (iso.length >= 16 ? iso.slice(11, 16) : iso);
  const title =
    unique.length === 1
      ? `${hhmm(first.t)} · ${Math.round(first.v)}%`
      : `${hhmm(first.t)} → ${hhmm(lastP.t)} · ${Math.round(first.v)}% → ${Math.round(last)}%`;
  return (
    <svg
      viewBox={`0 0 ${w} ${h}`}
      preserveAspectRatio="none"
      className="h-8 min-w-24 flex-1"
      aria-hidden
    >
      <title>{title}</title>
      {plotted.slice(1).map((point, index) => {
        const previous = plotted[index];
        return (
          <line
            key={`${previous.t}-${point.t}-${index}`}
            x1={x(index)}
            y1={y(previous.v)}
            x2={x(index + 1)}
            y2={y(point.v)}
            stroke={availColor(point.v).stroke}
            strokeWidth="2"
            strokeLinejoin="round"
            strokeLinecap="round"
            vectorEffect="non-scaling-stroke"
          />
        );
      })}
      <circle
        cx={x(plotted.length - 1)}
        cy={y(last)}
        r="2.2"
        fill={availColor(last).stroke}
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  );
}

function AvailabilityTrend({ value, trend }: { value: number; trend?: TrendPoint[] | null }) {
  const { t } = useI18n();
  const pct = Math.min(100, Math.max(0, value <= 1 ? value * 100 : value));
  const color = availColor(pct);
  const points = trend && trend.length > 0 ? trend : [{ t: "", v: pct }];
  const trendKey = points.map((point) => `${point.t}:${point.v}`).join("|");
  return (
    <div className="mt-1.5 flex items-center gap-2 pl-4 text-[11px]">
      <div className="flex w-24 shrink-0 items-center gap-1.5">
        <span className="text-gray-500 dark:text-gray-400">{t("list.successRate")}</span>
        <span className={`rounded px-1.5 py-0.5 font-semibold ${color.badge}`}>
          {Math.round(pct)}%
        </span>
      </div>
      <Sparkline key={trendKey} points={points} />
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
        const status = statusVisual(channel, t);
        return (
          <li
            key={`${channel.name}-${channel.model ?? ""}-${index}`}
            className="rounded border border-gray-100 bg-gray-50 px-2 py-1.5 text-sm text-gray-700 dark:border-gray-800 dark:bg-gray-800/60 dark:text-gray-200"
          >
            <div className="flex items-center gap-2">
              <i className={`h-2 w-2 shrink-0 rounded-full ${status.dot}`} />
              {provider && <ProviderIcon provider={provider} size={14} />}
              <span className="min-w-0 truncate font-medium">{channel.name}</span>
              <span className={`shrink-0 rounded px-1.5 py-0.5 text-[10px] ${status.badge}`}>
                {status.label}
              </span>
              {channel.plan_level && (
                <span className="shrink-0 rounded bg-gray-200 px-1.5 py-0.5 text-[10px] text-gray-600 dark:bg-gray-700 dark:text-gray-300">
                  {channel.plan_level}
                </span>
              )}
            </div>
            {channel.detail && (
              <p className="mt-0.5 pl-4 text-xs leading-5 text-gray-400">{channel.detail}</p>
            )}
            <div className="mt-1 flex flex-wrap gap-1 pl-4 text-[10px] text-gray-500 dark:text-gray-400">
              <span className="rounded bg-gray-200 px-1.5 py-0.5 dark:bg-gray-700">
                {t("list.modelRatio")}{" "}
                {channel.model_ratio == null ? "--" : `${formatRatio(channel.model_ratio)}x`}
              </span>
            </div>
            {channel.availability != null && (
              <AvailabilityTrend value={channel.availability} trend={channel.trend} />
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
