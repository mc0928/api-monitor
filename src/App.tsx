import { useCallback, useEffect, useMemo, useState } from "react";
import ProviderFilter from "./components/ProviderFilter";
import SettingsDialog from "./components/SettingsDialog";
import SiteCard from "./components/SiteCard";
import { getConfig, getResults, refreshAll, refreshSite, saveConfig } from "./lib/api";
import { bestAvailability, filterChannels, normalizeModels } from "./lib/models";
import type { AppConfig, ProviderId, SiteResult } from "./types";

export default function App() {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [results, setResults] = useState<Record<string, SiteResult>>({});
  const [refreshingAll, setRefreshingAll] = useState(false);
  const [refreshingIds, setRefreshingIds] = useState<Set<string>>(new Set());
  const [dialog, setDialog] = useState<
    null | { mode: "add" } | { mode: "edit"; siteId: string } | { mode: "settings" }
  >(null);
  const [provider, setProvider] = useState<ProviderId | "all">("all");
  const [loadError, setLoadError] = useState<string | null>(null);

  const applyResults = (list: SiteResult[]) => {
    setResults((prev) => {
      const next = { ...prev };
      for (const r of list) next[r.id] = r;
      return next;
    });
  };

  const applyConfig = (cfg: AppConfig) => {
    setConfig(cfg);
    const ids = new Set(cfg.sites.map((s) => s.id));
    setResults((prev) => {
      const next: Record<string, SiteResult> = {};
      for (const [id, result] of Object.entries(prev)) {
        if (ids.has(id)) next[id] = result;
      }
      return next;
    });
  };

  const load = useCallback(async () => {
    try {
      const [cfg, res] = await Promise.all([getConfig(), getResults()]);
      setConfig({
        ...cfg,
        monitor: { models: normalizeModels(cfg.monitor?.models) },
      });
      setResults(Object.fromEntries(res.map((r) => [r.id, r])));
      setLoadError(null);
    } catch (e) {
      setLoadError(String(e));
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      await load();
      if (cancelled) return;
      setRefreshingAll(true);
      try {
        applyResults(await refreshAll());
      } catch (e) {
        if (!cancelled) setLoadError(String(e));
      } finally {
        if (!cancelled) setRefreshingAll(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [load]);

  const handleRefreshAll = async () => {
    setRefreshingAll(true);
    try {
      applyResults(await refreshAll());
    } catch (e) {
      setLoadError(String(e));
    } finally {
      setRefreshingAll(false);
    }
  };

  const handleRemoveSite = async (id: string) => {
    if (!config) return;
    if (!window.confirm("确定移除该渠道？")) return;
    try {
      const next: AppConfig = {
        ...config,
        sites: config.sites.filter((s) => s.id !== id),
      };
      await saveConfig(next);
      applyConfig(next);
    } catch (e) {
      setLoadError(String(e));
    }
  };

  /** 设置弹窗保存后：写入配置；新增/编辑刷新单站，设置项变更刷新全部 */
  const handleSaved = (cfg: AppConfig, refreshId: string) => {
    applyConfig(cfg);
    setDialog(null);
    if (refreshId === "all") {
      void handleRefreshAll();
      return;
    }
    setResults((prev) => {
      const next = { ...prev };
      delete next[refreshId];
      return next;
    });
    void handleRefreshSite(refreshId);
  };

  const handleRefreshSite = async (id: string) => {
    setRefreshingIds((prev) => new Set(prev).add(id));
    try {
      applyResults([await refreshSite(id)]);
    } catch (e) {
      setLoadError(String(e));
    } finally {
      setRefreshingIds((prev) => {
        const next = new Set(prev);
        next.delete(id);
        return next;
      });
    }
  };

  const summary = useMemo(() => {
    const sites = config?.sites ?? [];
    const checked = sites.filter((s) => results[s.id]?.checked_at);
    const ok = checked.filter((s) => results[s.id].ok).length;
    return { total: sites.length, ok, fail: checked.length - ok, unchecked: sites.length - checked.length };
  }, [config, results]);

  const rankedSites = useMemo(() => {
    const sites = config?.sites ?? [];
    return [...sites].sort((a, b) => {
      const score = (id: string) =>
        bestAvailability(filterChannels(results[id]?.channels ?? [], provider));
      return score(b.id) - score(a.id);
    });
  }, [config, results, provider]);

  return (
    <div className="min-h-screen">
      {/* 顶栏：状态汇总 + 操作 */}
      <header className="sticky top-0 z-10 border-b border-gray-200 bg-white/90 backdrop-blur">
        <div className="mx-auto flex max-w-5xl items-center gap-4 px-6 py-4">
          <h1 className="text-lg font-semibold text-gray-900">API Monitor</h1>
          <div className="flex items-center gap-3 text-sm">
            <span className="flex items-center gap-1.5 text-emerald-600">
              <i className="h-2 w-2 rounded-full bg-emerald-500" />
              正常 {summary.ok}
            </span>
            <span className="flex items-center gap-1.5 text-red-600">
              <i className="h-2 w-2 rounded-full bg-red-500" />
              异常 {summary.fail}
            </span>
            {summary.unchecked > 0 && (
              <span className="flex items-center gap-1.5 text-gray-400">
                <i className="h-2 w-2 rounded-full bg-gray-300" />
                未检查 {summary.unchecked}
              </span>
            )}
          </div>
          <div className="ml-auto flex items-center gap-2">
            <button
              onClick={handleRefreshAll}
              disabled={refreshingAll || !config}
              className="rounded-lg bg-blue-600 px-4 py-1.5 text-sm font-medium text-white hover:bg-blue-500 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {refreshingAll ? "刷新中…" : "全部刷新"}
            </button>
            <button
              onClick={() => setDialog({ mode: "add" })}
              className="rounded-lg border border-gray-300 px-4 py-1.5 text-sm text-gray-600 hover:bg-gray-100"
            >
              添加渠道
            </button>
            <button
              onClick={() => setDialog({ mode: "settings" })}
              className="rounded-lg border border-gray-300 px-4 py-1.5 text-sm text-gray-600 hover:bg-gray-100"
            >
              设置
            </button>
          </div>
        </div>
        <div className="mx-auto flex max-w-5xl items-center gap-3 px-6 pb-3">
          <ProviderFilter value={provider} onChange={setProvider} />
          <span className="text-xs text-gray-400">默认按成功率排名</span>
        </div>
      </header>

      <main className="mx-auto max-w-5xl px-6 py-6">
        {loadError && (
          <div className="mb-4 rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-600">
            {loadError}
          </div>
        )}

        {!config && !loadError && <p className="text-gray-400">加载配置中…</p>}

        {config && config.sites.length === 0 && (
          <div className="rounded-lg border border-dashed border-gray-300 p-10 text-center text-gray-400">
            还没有渠道，点击「添加渠道」开始监控
          </div>
        )}

        <div className="grid gap-4 md:grid-cols-2">
          {rankedSites.map((site, index) => (
            <SiteCard
              key={site.id}
              site={site}
              result={results[site.id]}
              rank={index + 1}
              provider={provider}
              busy={refreshingIds.has(site.id) || refreshingAll}
              onRefresh={() => handleRefreshSite(site.id)}
              onEdit={() => setDialog({ mode: "edit", siteId: site.id })}
              onRemove={() => handleRemoveSite(site.id)}
            />
          ))}
        </div>
      </main>

      {dialog && (
        <SettingsDialog
          mode={dialog.mode}
          siteId={dialog.mode === "edit" ? dialog.siteId : undefined}
          onClose={() => setDialog(null)}
          onSaved={handleSaved}
        />
      )}
    </div>
  );
}
