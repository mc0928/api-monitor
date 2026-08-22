import { useEffect, useState } from "react";
import { getConfig, saveConfig, testProxy } from "../lib/api";
import { errMsg } from "../lib/errors";
import { useI18n } from "../lib/i18n";
import { PROVIDER_META, PROVIDERS, normalizeModels } from "../lib/models";
import type { AppConfig, ProviderId, SiteConfig, SiteType } from "../types";
import { ProviderIcon } from "./ProviderIcons";

interface Props {
  mode: "add" | "edit" | "settings";
  siteId?: string;
  onClose: () => void;
  onSaved: (config: AppConfig, refreshId: string | "all") => void;
}

const inputClass =
  "w-full rounded-lg border border-gray-300 bg-white px-3 py-1.5 text-sm text-gray-800 outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-100 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-100 dark:focus:ring-blue-900";

/** 自动刷新间隔选项：0 = 关闭
 */
const INTERVAL_OPTIONS = [0, 5, 10, 30];

function newSite(): SiteConfig {
  return {
    id: `site-${Date.now().toString(36)}`,
    name: "",
    type: "new2api",
    base_url: "",
    vpn: false,
  };
}

export default function SettingsDialog({ mode, siteId, onClose, onSaved }: Props) {
  const { t } = useI18n();
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [site, setSite] = useState<SiteConfig | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [proxyTip, setProxyTip] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const models = config ? normalizeModels(config.monitor?.models) : null;
  const intervalMinutes = config?.refresh?.interval_minutes ?? 5;

  useEffect(() => {
    getConfig()
      .then((cfg) => {
        setConfig({ ...cfg, monitor: { models: normalizeModels(cfg.monitor?.models) } });
        if (mode === "edit") {
          const found = cfg.sites.find((s) => s.id === siteId);
          if (!found) {
            setError(t("dialog.notFound"));
            return;
          }
          setSite({ ...found });
        } else if (mode === "add") {
          setSite(newSite());
        } else {
          setSite(null);
        }
      })
      .catch((e) => setError(errMsg(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode, siteId]);

  const patchSite = (patch: Partial<SiteConfig>) => {
    setSite((prev) => (prev ? { ...prev, ...patch } : prev));
  };

  const handleTestProxy = async () => {
    setProxyTip(t("dialog.testing"));
    try {
      setProxyTip(await testProxy());
    } catch (e) {
      setProxyTip(errMsg(e));
    }
  };

  const handleSave = async () => {
    if (!config || !models) return;
    if (mode !== "settings") {
      if (!site || !site.name.trim() || !site.base_url.trim()) {
        setError(t("dialog.needNameUrl"));
        return;
      }
    }
    setSaving(true);
    setError(null);
    try {
      const next: AppConfig = {
        ...config,
        monitor: { models },
        sites:
          mode === "settings"
            ? config.sites
            : mode === "add"
              ? [...config.sites, site!]
              : config.sites.map((s) => (s.id === site!.id ? site! : s)),
      };
      await saveConfig(next);
      onSaved(next, mode === "settings" ? "all" : site!.id);
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setSaving(false);
    }
  };

  const patchModels = (provider: ProviderId, next: string[]) => {
    setConfig((prev) =>
      prev
        ? { ...prev, monitor: { models: { ...prev.monitor.models, [provider]: next } } }
        : prev,
    );
  };

  const handleDelete = async () => {
    if (!config || !site) return;
    setSaving(true);
    setError(null);
    try {
      const next: AppConfig = {
        ...config,
        sites: config.sites.filter((s) => s.id !== site.id),
      };
      await saveConfig(next);
      onSaved(next, "all");
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-20 flex items-center justify-center bg-black/30 p-4"
      onClick={onClose}
    >
      <div
        className="flex max-h-[85vh] w-full max-w-2xl flex-col rounded-xl border border-gray-200 bg-white shadow-xl dark:border-gray-700 dark:bg-gray-900"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between border-b border-gray-200 px-5 py-3 dark:border-gray-700">
          <h2 className="font-semibold text-gray-900 dark:text-gray-100">
            {mode === "add"
              ? t("dialog.addTitle")
              : mode === "settings"
                ? t("dialog.settingsTitle")
                : t("dialog.editTitle")}
          </h2>
          <button
            onClick={onClose}
            className="text-gray-400 hover:text-gray-600 dark:hover:text-gray-200"
          >
            ✕
          </button>
        </div>

        <div className="space-y-6 overflow-auto px-5 py-4">
          {error && (
            <div className="rounded-lg border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-600 dark:border-red-900 dark:bg-red-950/40 dark:text-red-400">
              {error}
            </div>
          )}

          {!config && !error && <p className="text-gray-400">{t("dialog.loading")}</p>}

          {config && (mode === "settings" || site) && (
            <>
              {mode === "settings" && (
                <section>
                  <h3 className="mb-2 text-sm font-medium text-gray-700 dark:text-gray-300">
                    {t("dialog.autoRefresh")}
                  </h3>
                  <div className="flex flex-wrap items-center gap-2">
                    {INTERVAL_OPTIONS.map((minutes) => (
                      <button
                        key={minutes}
                        type="button"
                        onClick={() =>
                          setConfig({ ...config, refresh: { interval_minutes: minutes } })
                        }
                        className={`rounded-lg border px-3 py-1.5 text-sm transition ${
                          intervalMinutes === minutes
                            ? "border-blue-600 bg-blue-600 text-white"
                            : "border-gray-300 text-gray-600 hover:bg-gray-50 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-800"
                        }`}
                      >
                        {minutes === 0
                          ? t("dialog.off")
                          : t("dialog.intervalMinutes", { n: minutes })}
                      </button>
                    ))}
                  </div>
                  <label className="mt-3 flex cursor-pointer items-center gap-1.5 text-sm text-gray-600 dark:text-gray-300">
                    <input
                      type="checkbox"
                      checked={config.debug ?? false}
                      onChange={(e) => setConfig({ ...config, debug: e.target.checked })}
                    />
                    {t("dialog.debug")}
                    <span className="text-xs text-gray-400">{t("dialog.debugHint")}</span>
                  </label>
                </section>
              )}

              <section>
                <h3 className="mb-2 text-sm font-medium text-gray-700 dark:text-gray-300">
                  {t("dialog.proxy")}
                </h3>
                <div className="flex items-center gap-2">
                  <input
                    className={inputClass}
                    value={config.proxy.url}
                    placeholder="http://127.0.0.1:7897"
                    onChange={(e) =>
                      setConfig({ ...config, proxy: { url: e.target.value } })
                    }
                  />
                  <button
                    onClick={handleTestProxy}
                    className="shrink-0 rounded-lg border border-gray-300 px-3 py-1.5 text-sm text-gray-600 hover:bg-gray-50 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-800"
                  >
                    {t("dialog.test")}
                  </button>
                </div>
                {proxyTip && <p className="mt-1 text-xs text-gray-400">{proxyTip}</p>}
              </section>

              <section>
                <div className="mb-2 flex items-center justify-between">
                  <h3 className="text-sm font-medium text-gray-700 dark:text-gray-300">
                    {t("dialog.monitoredModels")}
                  </h3>
                  <button
                    type="button"
                    className="text-xs text-gray-400 hover:text-gray-600 dark:hover:text-gray-200"
                    onClick={() =>
                      setConfig({
                        ...config,
                        monitor: { models: normalizeModels(null) },
                      })
                    }
                  >
                    {t("dialog.resetDefaults")}
                  </button>
                </div>
                <p className="mb-2 text-xs text-gray-400">{t("dialog.modelsHint")}</p>
                <div className="space-y-3">
                  {PROVIDERS.map((id) => (
                    <ModelChips
                      key={id}
                      provider={id}
                      models={models?.[id] ?? []}
                      onChange={(list) => patchModels(id, list)}
                    />
                  ))}
                </div>
              </section>

              {site && (
                <section>
                  <h3 className="mb-2 text-sm font-medium text-gray-700 dark:text-gray-300">
                    {t("dialog.channel")}
                  </h3>
                  <div className="grid grid-cols-2 gap-2">
                    <input
                      className={inputClass}
                      value={site.name}
                      placeholder={t("dialog.name")}
                      onChange={(e) => patchSite({ name: e.target.value })}
                    />
                    <select
                      className={inputClass}
                      value={site.type}
                      onChange={(e) => patchSite({ type: e.target.value as SiteType })}
                    >
                      <option value="new2api">{t("dialog.typeNew2api")}</option>
                      <option value="sub2api">{t("dialog.typeSub2api")}</option>
                    </select>
                    <input
                      className={`${inputClass} col-span-2`}
                      value={site.base_url}
                      placeholder="https://example.com"
                      onChange={(e) => patchSite({ base_url: e.target.value })}
                    />

                    {site.type === "new2api" && (
                      <>
                        <input
                          className={`${inputClass} col-span-2`}
                          value={site.token ?? ""}
                          placeholder={t("dialog.token")}
                          onChange={(e) => patchSite({ token: e.target.value })}
                        />
                        <p className="col-span-2 -mt-1 text-xs text-gray-400">
                          {t("dialog.tokenHint")}
                        </p>
                        <input
                          className={`${inputClass} col-span-2`}
                          value={site.user_id ?? ""}
                          placeholder={t("dialog.userId")}
                          onChange={(e) => patchSite({ user_id: e.target.value })}
                        />
                      </>
                    )}
                    {site.type === "sub2api" && (
                      <>
                        <input
                          className={inputClass}
                          value={site.username ?? ""}
                          placeholder={t("dialog.username")}
                          onChange={(e) => patchSite({ username: e.target.value })}
                        />
                        <input
                          className={inputClass}
                          type="password"
                          value={site.password ?? ""}
                          placeholder={t("dialog.password")}
                          onChange={(e) => patchSite({ password: e.target.value })}
                        />
                      </>
                    )}
                  </div>

                  <div className="mt-2 flex items-center gap-3 text-sm">
                    <label className="flex cursor-pointer items-center gap-1.5 text-gray-600 dark:text-gray-300">
                      <input
                        type="checkbox"
                        checked={site.vpn}
                        onChange={(e) => patchSite({ vpn: e.target.checked })}
                      />
                      {t("dialog.vpn")}
                    </label>
                    {mode === "edit" && (
                      <button
                        onClick={handleDelete}
                        disabled={saving}
                        className="ml-auto text-xs text-red-500 hover:text-red-600 dark:text-red-400 dark:hover:text-red-300"
                      >
                        {t("dialog.delete")}
                      </button>
                    )}
                  </div>
                </section>
              )}
            </>
          )}
        </div>

        <div className="flex justify-end gap-2 border-t border-gray-200 px-5 py-3 dark:border-gray-700">
          <button
            onClick={onClose}
            className="rounded-lg border border-gray-300 px-4 py-1.5 text-sm text-gray-600 hover:bg-gray-50 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-800"
          >
            {t("dialog.cancel")}
          </button>
          <button
            onClick={handleSave}
            disabled={!config || (mode !== "settings" && !site) || saving}
            className="rounded-lg bg-blue-600 px-4 py-1.5 text-sm font-medium text-white hover:bg-blue-500 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {saving
              ? t("dialog.saving")
              : mode === "add"
                ? t("dialog.add")
                : t("dialog.save")}
          </button>
        </div>
      </div>
    </div>
  );
}

function ModelChips({
  provider,
  models,
  onChange,
}: {
  provider: ProviderId;
  models: string[];
  onChange: (models: string[]) => void;
}) {
  const { t } = useI18n();
  const [draft, setDraft] = useState("");
  const add = () => {
    const name = draft.trim();
    if (!name) return;
    if (!models.includes(name)) onChange([...models, name]);
    setDraft("");
  };
  return (
    <div>
      <div className="mb-1.5 flex items-center gap-1.5 text-xs font-medium text-gray-600 dark:text-gray-300">
        <ProviderIcon provider={provider} size={14} />
        {PROVIDER_META[provider].label}
      </div>
      <div className="flex flex-wrap items-center gap-1.5">
        {models.map((model) => (
          <span
            key={model}
            className="inline-flex items-center gap-1 rounded-full bg-gray-100 px-2 py-0.5 text-xs text-gray-700 dark:bg-gray-800 dark:text-gray-200"
          >
            {model}
            <button
              type="button"
              className="text-gray-400 hover:text-red-500 dark:hover:text-red-400"
              onClick={() => onChange(models.filter((m) => m !== model))}
            >
              ×
            </button>
          </span>
        ))}
        <input
          className="min-w-32 flex-1 rounded border border-gray-200 bg-white px-2 py-0.5 text-xs outline-none focus:border-blue-400 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-100"
          value={draft}
          placeholder={t("dialog.addModelHint")}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              add();
            }
          }}
          onBlur={add}
        />
      </div>
    </div>
  );
}
