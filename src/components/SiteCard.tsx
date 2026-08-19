import { filterChannels } from "../lib/models";
import type { ProviderId, SiteConfig, SiteResult } from "../types";
import ChannelList from "./ChannelList";

interface Props {
  site: SiteConfig;
  result?: SiteResult;
  busy: boolean;
  rank?: number;
  provider: ProviderId | "all";
  onRefresh: () => void;
  onEdit: () => void;
  onRemove: () => void;
}

function ChannelHealthBar({
  label,
  total,
  operational,
  degraded,
  failed,
}: {
  label: string;
  total: number;
  operational: number;
  degraded: number;
  failed: number;
}) {
  const pct = (n: number) => (total > 0 ? `${(n / total) * 100}%` : "0%");
  return (
    <div>
      <div className="flex h-2 overflow-hidden rounded-full bg-gray-200">
        {operational > 0 && (
          <div className="bg-emerald-500" style={{ width: pct(operational) }} />
        )}
        {degraded > 0 && <div className="bg-amber-500" style={{ width: pct(degraded) }} />}
        {failed > 0 && <div className="bg-red-500" style={{ width: pct(failed) }} />}
      </div>
      <p className="mt-1 text-xs text-gray-400">
        {label} {total} · 正常 {operational}
        {degraded > 0 ? ` · 降级 ${degraded}` : ""}
        {failed > 0 ? ` · 故障 ${failed}` : ""}
      </p>
    </div>
  );
}

export default function SiteCard({
  site,
  result,
  busy,
  rank,
  provider,
  onRefresh,
  onEdit,
  onRemove,
}: Props) {
  const statusDot = !result?.checked_at
    ? "bg-gray-300"
    : result.ok
      ? "bg-emerald-500"
      : "bg-red-500";

  const channels = filterChannels(result?.channels ?? [], provider);
  const operationalCount = channels.filter(
    (c) => c.status === "operational" || (c.online && c.status !== "degraded"),
  ).length;
  const degradedCount = channels.filter((c) => c.status === "degraded").length;
  const failedCount = Math.max(0, channels.length - operationalCount - degradedCount);

  return (
    <div className="rounded-xl border border-gray-200 bg-white p-4 shadow-sm">
      {/* 卡片头：状态点 + 名称 + 徽标 + 刷新 */}
      <div className="flex items-center gap-2">
        {rank != null && (
          <span className="w-5 shrink-0 text-center text-xs font-semibold text-gray-400">
            {rank}
          </span>
        )}
        <i className={`h-2.5 w-2.5 shrink-0 rounded-full ${statusDot}`} />
        <span className="truncate font-medium text-gray-900">{site.name}</span>
        <span className="rounded bg-gray-100 px-1.5 py-0.5 text-xs text-gray-500">
          {site.type}
        </span>
        {site.vpn && (
          <span className="rounded bg-blue-50 px-1.5 py-0.5 text-xs text-blue-600">
            代理
          </span>
        )}
        <div className="ml-auto flex shrink-0 items-center gap-1">
          <button
            onClick={onEdit}
            disabled={busy}
            className="rounded border border-gray-300 px-2 py-0.5 text-xs text-gray-600 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50"
          >
            修改配置
          </button>
          <button
            onClick={onRefresh}
            disabled={busy}
            className="rounded border border-gray-300 px-2 py-0.5 text-xs text-gray-600 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {busy ? "刷新中…" : "刷新"}
          </button>
          <button
            onClick={onRemove}
            disabled={busy}
            className="rounded border border-red-200 px-2 py-0.5 text-xs text-red-500 hover:bg-red-50 disabled:cursor-not-allowed disabled:opacity-50"
          >
            移除
          </button>
        </div>
      </div>

      <p className="mt-1 truncate text-xs text-gray-400" title={site.base_url}>
        {site.base_url}
      </p>

      <div className="mt-3 min-h-10">
        {!result?.checked_at && <p className="text-sm text-gray-400">尚未检查</p>}

        {result && result.checked_at > 0 && (
          <>
            {result.error && <p className="text-sm text-red-600">{result.error}</p>}
            {result.note && <p className="text-xs text-amber-600">{result.note}</p>}

            {/* 站点余额：new2api 账户余额，或 sub2api 渠道 USD 合计 */}
            {result.ok && result.balance_usd != null && (
              <div className="flex flex-wrap gap-x-6 gap-y-1 text-sm">
                <div>
                  <span className="text-gray-400">余额 </span>
                  <span
                    className={`font-medium ${
                      result.balance_usd <= 0 ? "text-red-600" : "text-emerald-600"
                    }`}
                  >
                    ${result.balance_usd.toFixed(2)}
                  </span>
                </div>
              </div>
            )}

            {/* 渠道状态进度条 + 列表（new2api / sub2api 通用） */}
            {result.ok && channels.length > 0 && (
              <div className="mt-3">
                <ChannelHealthBar
                  label="渠道"
                  total={channels.length}
                  operational={operationalCount}
                  degraded={degradedCount}
                  failed={failedCount}
                />
                <div className="mt-2">
                  <ChannelList channels={channels} />
                </div>
              </div>
            )}

            {result.ok && (result.channels?.length ?? 0) > 0 && channels.length === 0 && (
              <p className="mt-3 text-sm text-gray-400">该渠道暂无此模型数据</p>
            )}

            <div className="mt-2 flex items-center gap-2">
              <p className="text-xs text-gray-400">
                检查于 {new Date(result.checked_at).toLocaleTimeString()}
              </p>
              {site.type !== "sub2api" && result.raw && (
                <details className="text-xs text-gray-400">
                  <summary className="cursor-pointer select-none hover:text-gray-600">
                    原始响应
                  </summary>
                  <pre className="mt-1 max-h-40 overflow-auto whitespace-pre-wrap rounded bg-gray-50 p-2 text-[11px] leading-relaxed text-gray-500">
                    {result.raw}
                  </pre>
                </details>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
