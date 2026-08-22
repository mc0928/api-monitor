import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import ProviderFilter from "./components/ProviderFilter";
import SettingsDialog from "./components/SettingsDialog";
import SiteCard from "./components/SiteCard";
import { getConfig, getResults, refreshAll, refreshSite, saveConfig } from "./lib/api";
import { errMsg } from "./lib/errors";
import { useI18n } from "./lib/i18n";
import { bestAvailability, filterChannels, normalizeModels } from "./lib/models";
import type { AppConfig, ProviderId, SiteResult } from "./types";

const THEME_KEY = "api-monitor:theme";

function initialTheme(): "light" | "dark" {
  const saved = localStorage.getItem(THEME_KEY);
  if (saved === "dark" || saved === "light") return saved;
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

export default function App() {
  const { t, lang, setLang } = useI18n();
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [results, setResults] = useState<Record<string, SiteResult>>({});
  const [refreshingAll, setRefreshingAll] = useState(false);
  const [refreshingIds, setRefreshingIds] = useState<Set<string>>(new Set());
  const [dialog, setDialog] = useState<
    null | { mode: "add" } | { mode: "edit"; siteId: string } | { mode: "settings" }
  >(null);
  const [provider, setProvider] = useState<ProviderId | "all">("all");
  const [loadError, setLoadError] = useState<string | null>(null);
  const [theme, setTheme] = useState<"light" | "dark">(initialTheme);

  // 通知权限与上轮结果快照（用于刷新后对比出状态变化）
  const notifyAllowedRef = useRef(false);
  const prevResultsRef = useRef<Record<string, SiteResult>>({});
  const refreshingRef = useRef(false);

  useEffect(() => {
    document.documentElement.classList.toggle("dark", theme === "dark");
    localStorage.setItem(THEME_KEY, theme);
  }, [theme]);

  useEffect(() => {
    void (async () => {
      let granted = await isPermissionGranted();
      if (!granted) granted = (await requestPermission()) === "granted";
      notifyAllowedRef.current = granted;
    })();
  }, []);

  /** 对比上一轮结果，状态恶化/恢复时弹系统通知 */
  const notifyChanges = (list: SiteResult[]) => {
    if (!notifyAllowedRef.current) return;
    for (const next of list) {
      const prev = prevResultsRef.current[next.id];
      if (!prev || prev.checked_at === next.checked_at) continue;
      if (prev.ok && !next.ok) {
        sendNotification({
          title: t("notif.siteFailed", { name: next.name }),
          body: errMsg(next.error).slice(0, 120),
        });
        continue;
      }
      if (!prev.ok && next.ok) {
        sendNotification({ title: t("notif.siteRecovered", { name: next.name }) });
        continue;
      }
      if (prev.ok && next.ok) {
        const prevFailed = new Set(
          prev.channels.filter((c) => c.status === "failed").map((c) => c.name),
        );
        const newFailed = next.channels.filter(
          (c) => c.status === "failed" && !prevFailed.has(c.name),
        );
        if (newFailed.length > 0) {
          const names = newFailed.slice(0, 3).map((c) => c.name).join("、");
          const body =
            newFailed.length > 3
              ? `${names} ${t("notif.andMore", { count: newFailed.length })}`
              : names;
          sendNotification({
            title: t("notif.channelsFailed", { name: next.name }),
            body,
          });
        }
      }
    }
  };

  const applyResults = (list: SiteResult[]) => {
    notifyChanges(list);
    for (const r of list) prevResultsRef.current[r.id] = r;
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
      // 持久化恢复的上次结果只入快照，不触发通知
      const map = Object.fromEntries(res.map((r) => [r.id, r]));
      prevResultsRef.current = map;
      setResults(map);
      setLoadError(null);
    } catch (e) {
      setLoadError(errMsg(e));
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
        if (!cancelled) setLoadError(errMsg(e));
      } finally {
        if (!cancelled) setRefreshingAll(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [load]);

  const handleRefreshAll = useCallback(async () => {
    if (refreshingRef.current) return;
    refreshingRef.current = true;
    setRefreshingAll(true);
    try {
      applyResults(await refreshAll());
    } catch (e) {
      setLoadError(errMsg(e));
    } finally {
      setRefreshingAll(false);
      refreshingRef.current = false;
    }
  }, [t]);

  // 自动刷新：0 = 关闭；窗口隐藏到托盘时定时器可能被节流，分钟级间隔不受实质影响
  const intervalMinutes = config?.refresh?.interval_minutes ?? 5;
  useEffect(() => {
    if (!intervalMinutes) return;
    const id = setInterval(() => void handleRefreshAll(), intervalMinutes * 60_000);
    return () => clearInterval(id);
  }, [intervalMinutes, handleRefreshAll]);

  const handleRemoveSite = async (id: string) => {
    if (!config) return;
    if (!window.confirm(t("confirm.remove"))) return;
    try {
      const next: AppConfig = {
        ...config,
        sites: config.sites.filter((s) => s.id !== id),
      };
      await saveConfig(next);
      applyConfig(next);
    } catch (e) {
      setLoadError(errMsg(e));
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
      setLoadError(errMsg(e));
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
      <header className="sticky top-0 z-10 border-b border-gray-200 bg-white/90 backdrop-blur dark:border-gray-800 dark:bg-gray-900/90">
        <div className="mx-auto flex max-w-5xl items-center gap-4 px-6 py-4">
          <h1 className="text-lg font-semibold text-gray-900 dark:text-gray-100">API Monitor</h1>
          <div className="flex items-center gap-3 text-sm">
            <span className="flex items-center gap-1.5 text-emerald-600 dark:text-emerald-400">
              <i className="h-2 w-2 rounded-full bg-emerald-500" />
              {t("status.ok")} {summary.ok}
            </span>
            <span className="flex items-center gap-1.5 text-red-600 dark:text-red-400">
              <i className="h-2 w-2 rounded-full bg-red-500" />
              {t("status.fail")} {summary.fail}
            </span>
            {summary.unchecked > 0 && (
              <span className="flex items-center gap-1.5 text-gray-400">
                <i className="h-2 w-2 rounded-full bg-gray-300 dark:bg-gray-600" />
                {t("status.unchecked")} {summary.unchecked}
              </span>
            )}
          </div>
          <div className="ml-auto flex items-center gap-2">
            <button
              type="button"
              onClick={() => setLang(lang === "zh" ? "en" : "zh")}
              title={lang === "zh" ? "English" : "简体中文"}
              className="rounded-lg border border-gray-300 px-2.5 py-1.5 text-xs text-gray-600 hover:bg-gray-100 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-800"
            >
              {lang === "zh" ? "EN" : "中"}
            </button>
            <button
              type="button"
              onClick={() => setTheme(theme === "dark" ? "light" : "dark")}
              title={theme === "dark" ? "Light mode" : "深色模式"}
              className="rounded-lg border border-gray-300 px-2.5 py-1.5 text-xs text-gray-600 hover:bg-gray-100 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-800"
            >
              {theme === "dark" ? "☀️" : "🌙"}
            </button>
            <button
              onClick={handleRefreshAll}
              disabled={refreshingAll || !config}
              className="rounded-lg bg-blue-600 px-4 py-1.5 text-sm font-medium text-white hover:bg-blue-500 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {refreshingAll ? t("action.refreshing") : t("action.refreshAll")}
            </button>
            <button
              onClick={() => setDialog({ mode: "add" })}
              className="rounded-lg border border-gray-300 px-4 py-1.5 text-sm text-gray-600 hover:bg-gray-100 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-800"
            >
              {t("action.addSite")}
            </button>
            <button
              onClick={() => setDialog({ mode: "settings" })}
              className="rounded-lg border border-gray-300 px-4 py-1.5 text-sm text-gray-600 hover:bg-gray-100 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-800"
            >
              {t("action.settings")}
            </button>
          </div>
        </div>
        <div className="mx-auto flex max-w-5xl items-center gap-3 px-6 pb-3">
          <ProviderFilter value={provider} onChange={setProvider} />
          <span className="text-xs text-gray-400">{t("filter.rankHint")}</span>
        </div>
      </header>

      <main className="mx-auto max-w-5xl px-6 py-6">
        {loadError && (
          <div className="mb-4 rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-600 dark:border-red-900 dark:bg-red-950/40 dark:text-red-400">
            {loadError}
          </div>
        )}

        {!config && !loadError && <p className="text-gray-400">{t("load.loading")}</p>}

        {config && config.sites.length === 0 && (
          <div className="rounded-lg border border-dashed border-gray-300 p-10 text-center text-gray-400 dark:border-gray-700">
            {t("load.empty")}
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
