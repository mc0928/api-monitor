import { useEffect, useState } from "react";
import { getConfig, saveConfig, testProxy } from "../lib/api";
import {
  DEFAULT_MODELS,
  PROVIDER_META,
  PROVIDERS,
  normalizeModels,
} from "../lib/models";
import type { AppConfig, ProviderId, SiteConfig, SiteType } from "../types";
import { ProviderIcon } from "./ProviderIcons";

interface Props {
  mode: "add" | "edit" | "settings";
  siteId?: string;
  onClose: () => void;
  onSaved: (config: AppConfig, refreshId: string | "all") => void;
}

const inputClass =
  "w-full rounded-lg border border-gray-300 bg-white px-3 py-1.5 text-sm text-gray-800 outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-100";

function newSite(): SiteConfig {
  return {
    id: `site-${Date.now().toString(36)}`,
    name: "",
    type: "new2api",
    base_url: "",
    vpn: false,
    token: "",
    username: "",
    password: "",
  };
}

export default function SettingsDialog({ mode, siteId, onClose, onSaved }: Props) {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [site, setSite] = useState<SiteConfig | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [proxyTip, setProxyTip] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    getConfig()
      .then((cfg) => {
        setConfig({ ...cfg, monitor: { models: normalizeModels(cfg.monitor?.models) } });
        if (mode === "edit") {
          const found = cfg.sites.find((s) => s.id === siteId);
          if (!found) {
            setError("未找到该渠道");
            return;
          }
          setSite({ ...found });
        } else if (mode === "add") {
          setSite(newSite());
        } else {
          setSite(null);
        }
      })
      .catch((e) => setError(String(e)));
  }, [mode, siteId]);

  const patchSite = (patch: Partial<SiteConfig>) => {
    setSite((prev) => (prev ? { ...prev, ...patch } : prev));
  };

  const handleTestProxy = async () => {
    setProxyTip("测试中…");
    try {
      setProxyTip(await testProxy());
    } catch (e) {
      setProxyTip(String(e));
    }
  };

  const handleSave = async () => {
    if (!config) return;
    if (mode !== "settings") {
      if (!site || !site.name.trim() || !site.base_url.trim()) {
        setError("请填写名称和地址");
        return;
      }
    }
    setSaving(true);
    setError(null);
    try {
      const next: AppConfig = {
        ...config,
        monitor: { models: normalizeModels(config.monitor?.models) },
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
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  const patchModels = (provider: ProviderId, models: string[]) => {
    setConfig((prev) =>
      prev
        ? {
            ...prev,
            monitor: {
              models: {
                ...normalizeModels(prev.monitor?.models),
                [provider]: models,
              },
            },
          }
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
      setError(String(e));
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
        className="flex max-h-[85vh] w-full max-w-2xl flex-col rounded-xl border border-gray-200 bg-white shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between border-b border-gray-200 px-5 py-3">
          <h2 className="font-semibold text-gray-900">
            {mode === "add" ? "添加渠道" : mode === "settings" ? "设置" : "修改配置"}
          </h2>
          <button onClick={onClose} className="text-gray-400 hover:text-gray-600">
            ✕
          </button>
        </div>

        <div className="space-y-6 overflow-auto px-5 py-4">
          {error && (
            <div className="rounded-lg border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-600">
              {error}
            </div>
          )}

          {!config && !error && <p className="text-gray-400">加载中…</p>}

          {config && (mode === "settings" || site) && (
            <>
              <section>
                <h3 className="mb-2 text-sm font-medium text-gray-700">代理（Clash 混合代理）</h3>
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
                    className="shrink-0 rounded-lg border border-gray-300 px-3 py-1.5 text-sm text-gray-600 hover:bg-gray-50"
                  >
                    测试
                  </button>
                </div>
                {proxyTip && <p className="mt-1 text-xs text-gray-400">{proxyTip}</p>}
              </section>

              <section>
                <div className="mb-2 flex items-center justify-between">
                  <h3 className="text-sm font-medium text-gray-700">监控模型</h3>
                  <button
                    type="button"
                    className="text-xs text-gray-400 hover:text-gray-600"
                    onClick={() =>
                      setConfig({
                        ...config,
                        monitor: {
                          models: {
                            gpt: [...DEFAULT_MODELS.gpt],
                            claude: [...DEFAULT_MODELS.claude],
                            grok: [...DEFAULT_MODELS.grok],
                            kimi: [...DEFAULT_MODELS.kimi],
                          },
                        },
                      })
                    }
                  >
                    恢复默认
                  </button>
                </div>
                <p className="mb-2 text-xs text-gray-400">
                  列表展示渠道状态；筛选和 new2api 成功率优先按这些模型归类
                </p>
                <div className="space-y-3">
                  {PROVIDERS.map((id) => (
                    <ModelChips
                      key={id}
                      provider={id}
                      models={normalizeModels(config.monitor?.models)[id]}
                      onChange={(list) => patchModels(id, list)}
                    />
                  ))}
                </div>
              </section>

              {site && (
              <section>
                <h3 className="mb-2 text-sm font-medium text-gray-700">渠道</h3>
                <div className="grid grid-cols-2 gap-2">
                  <input
                    className={inputClass}
                    value={site.name}
                    placeholder="名称"
                    onChange={(e) => patchSite({ name: e.target.value })}
                  />
                  <select
                    className={inputClass}
                    value={site.type}
                    onChange={(e) => patchSite({ type: e.target.value as SiteType })}
                  >
                    <option value="new2api">new2api（令牌查余额）</option>
                    <option value="sub2api">sub2api（登录拉渠道）</option>
                  </select>
                  <input
                    className={`${inputClass} col-span-2`}
                    value={site.base_url}
                    placeholder="https://example.com"
                    onChange={(e) => patchSite({ base_url: e.target.value })}
                  />

                  {site.type === "new2api" && (
                    <input
                      className={`${inputClass} col-span-2`}
                      value={site.token ?? ""}
                      placeholder="个人访问令牌"
                      onChange={(e) => patchSite({ token: e.target.value })}
                    />
                  )}
                  {site.type === "sub2api" && (
                    <>
                      <input
                        className={inputClass}
                        value={site.username ?? ""}
                        placeholder="账号（邮箱）"
                        onChange={(e) => patchSite({ username: e.target.value })}
                      />
                      <input
                        className={inputClass}
                        type="password"
                        value={site.password ?? ""}
                        placeholder="密码"
                        onChange={(e) => patchSite({ password: e.target.value })}
                      />
                    </>
                  )}
                </div>

                <div className="mt-2 flex items-center gap-3 text-sm">
                  <label className="flex cursor-pointer items-center gap-1.5 text-gray-600">
                    <input
                      type="checkbox"
                      checked={site.vpn}
                      onChange={(e) => patchSite({ vpn: e.target.checked })}
                    />
                    走代理（vpn）
                  </label>
                  {mode === "edit" && (
                    <button
                      onClick={handleDelete}
                      disabled={saving}
                      className="ml-auto text-xs text-red-500 hover:text-red-600"
                    >
                      删除渠道
                    </button>
                  )}
                </div>
              </section>
              )}
            </>
          )}
        </div>

        <div className="flex justify-end gap-2 border-t border-gray-200 px-5 py-3">
          <button
            onClick={onClose}
            className="rounded-lg border border-gray-300 px-4 py-1.5 text-sm text-gray-600 hover:bg-gray-50"
          >
            取消
          </button>
          <button
            onClick={handleSave}
            disabled={!config || (mode !== "settings" && !site) || saving}
            className="rounded-lg bg-blue-600 px-4 py-1.5 text-sm font-medium text-white hover:bg-blue-500 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {saving ? "保存中…" : mode === "add" ? "添加" : "保存"}
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
  const [draft, setDraft] = useState("");
  const add = () => {
    const name = draft.trim();
    if (!name) return;
    if (!models.includes(name)) onChange([...models, name]);
    setDraft("");
  };
  return (
    <div>
      <div className="mb-1.5 flex items-center gap-1.5 text-xs font-medium text-gray-600">
        <ProviderIcon provider={provider} size={14} />
        {PROVIDER_META[provider].label}
      </div>
      <div className="flex flex-wrap items-center gap-1.5">
        {models.map((model) => (
          <span
            key={model}
            className="inline-flex items-center gap-1 rounded-full bg-gray-100 px-2 py-0.5 text-xs text-gray-700"
          >
            {model}
            <button
              type="button"
              className="text-gray-400 hover:text-red-500"
              onClick={() => onChange(models.filter((m) => m !== model))}
            >
              ×
            </button>
          </span>
        ))}
        <input
          className="min-w-32 flex-1 rounded border border-gray-200 bg-white px-2 py-0.5 text-xs outline-none focus:border-blue-400"
          value={draft}
          placeholder="添加模型名，回车"
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
