import { createContext, useContext, useState } from "react";
import type { ReactNode } from "react";

export type Lang = "zh" | "en";

const LANG_KEY = "api-monitor:lang";

/** 扁平 key 字典：zh 为源语言，en 一一对应 */
const zh: Record<string, string> = {
  "status.ok": "正常",
  "status.fail": "异常",
  "status.unchecked": "未检查",
  "action.refreshAll": "全部刷新",
  "action.refreshing": "刷新中…",
  "action.addSite": "添加渠道",
  "action.settings": "设置",
  "filter.all": "全部",
  "load.loading": "加载配置中…",
  "load.empty": "还没有渠道，点击「添加渠道」开始监控",
  "confirm.remove": "确定移除该渠道？",
  "card.notChecked": "尚未检查",
  "card.balance": "余额",
  "card.noModelData": "该渠道暂无此模型数据",
  "card.checkedAt": "检查于 {time}",
  "card.rawResponse": "原始响应",
  "card.edit": "修改配置",
  "card.refresh": "刷新",
  "card.remove": "移除",
  "card.proxy": "代理",
  "card.channels": "渠道",
  "card.normal": "正常",
  "card.degraded": "降级",
  "card.failed": "故障",
  "list.noData": "未解析到渠道数据",
  "list.successRate": "成功率",
  "list.balance": "余额",
  "list.modelRatio": "模型倍率",
  "list.resetSoon": "即将重置",
  "list.window.5h": "5小时",
  "list.window.7d": "7天",
  "list.window.7d-sonnet": "7天 Sonnet",
  "list.window.7d-fable": "7天 Fable",
  "list.window.weekly": "每周",
  "list.window.daily": "每日",
  "list.window.30d": "30天",
  "list.window.total": "总计",
  "list.tier.requests": "请求",
  "list.tier.tokens": "Token",
  "list.tier.shared": "共享",
  "list.tier.pro": "Pro",
  "list.tier.flash": "Flash",
  "dialog.addTitle": "添加渠道",
  "dialog.settingsTitle": "设置",
  "dialog.editTitle": "修改配置",
  "dialog.loading": "加载中…",
  "dialog.notFound": "未找到该渠道",
  "dialog.needNameUrl": "请填写名称和地址",
  "dialog.proxy": "代理（Clash 混合代理）",
  "dialog.test": "测试",
  "dialog.testing": "测试中…",
  "dialog.monitoredModels": "监控模型",
  "dialog.resetDefaults": "恢复默认",
  "dialog.modelsHint": "列表仅展示以上模型对应的渠道状态；gpt-image 归入 GPT，其他图片、Embedding 与 Reranker 模型不自动归类",
  "dialog.channel": "渠道",
  "dialog.name": "名称",
  "dialog.typeNew2api": "new2api（令牌查余额）",
  "dialog.typeSub2api": "sub2api（登录拉渠道）",
  "dialog.token": "个人访问令牌（可选）",
  "dialog.tokenHint": "留空则仅显示模型广场数据（不含余额）",
  "dialog.userId": "用户 ID（New-Api-User，可选）",
  "dialog.username": "账号（邮箱）",
  "dialog.password": "密码",
  "dialog.vpn": "走代理（vpn）",
  "dialog.delete": "删除渠道",
  "dialog.cancel": "取消",
  "dialog.save": "保存",
  "dialog.saving": "保存中…",
  "dialog.add": "添加",
  "dialog.addModelHint": "添加模型名，回车",
  "dialog.autoRefresh": "自动刷新",
  "dialog.off": "关闭",
  "dialog.intervalMinutes": "{n} 分钟",
  "dialog.debug": "调试模式",
  "dialog.debugHint": "在结果中保留原始响应数据",
  "dialog.cardSort": "卡片排序",
  "dialog.sortAuto": "自动（按成功率）",
  "dialog.sortManual": "手动（拖动排序）",
  "dialog.sortHint": "拖动 ⠿ 调整站点卡片显示顺序",
  "notif.siteFailed": "{name} 刷新失败",
  "notif.siteRecovered": "{name} 已恢复",
  "notif.channelsFailed": "{name} 渠道异常",
  "notif.andMore": "等 {count} 个渠道",
  "update.available": "新版本 {version} 可用",
  "update.download": "前往下载",
  "trend.tooltip": "近 24 小时成功率：{first} → {last}（{start} 起）",
};

const en: Record<string, string> = {
  "status.ok": "OK",
  "status.fail": "Failed",
  "status.unchecked": "Unchecked",
  "action.refreshAll": "Refresh all",
  "action.refreshing": "Refreshing…",
  "action.addSite": "Add site",
  "action.settings": "Settings",
  "filter.all": "All",
  "load.loading": "Loading config…",
  "load.empty": "No sites yet — click “Add site” to start monitoring",
  "confirm.remove": "Remove this site?",
  "card.notChecked": "Not checked yet",
  "card.balance": "Balance",
  "card.noModelData": "No data for this model family",
  "card.checkedAt": "Checked at {time}",
  "card.rawResponse": "Raw response",
  "card.edit": "Edit",
  "card.refresh": "Refresh",
  "card.remove": "Remove",
  "card.proxy": "Proxy",
  "card.channels": "Channels",
  "card.normal": "OK",
  "card.degraded": "Degraded",
  "card.failed": "Failed",
  "list.noData": "No channel data parsed",
  "list.successRate": "Success rate",
  "list.balance": "Balance",
  "list.modelRatio": "Model ratio",
  "list.resetSoon": "resetting soon",
  "list.window.5h": "5h",
  "list.window.7d": "7d",
  "list.window.7d-sonnet": "7d Sonnet",
  "list.window.7d-fable": "7d Fable",
  "list.window.weekly": "Weekly",
  "list.window.daily": "Daily",
  "list.window.30d": "30d",
  "list.window.total": "Total",
  "list.tier.requests": "Requests",
  "list.tier.tokens": "Tokens",
  "list.tier.shared": "Shared",
  "list.tier.pro": "Pro",
  "list.tier.flash": "Flash",
  "dialog.addTitle": "Add site",
  "dialog.settingsTitle": "Settings",
  "dialog.editTitle": "Edit site",
  "dialog.loading": "Loading…",
  "dialog.notFound": "Site not found",
  "dialog.needNameUrl": "Name and URL are required",
  "dialog.proxy": "Proxy (Clash mixed port)",
  "dialog.test": "Test",
  "dialog.testing": "Testing…",
  "dialog.monitoredModels": "Monitored models",
  "dialog.resetDefaults": "Reset to defaults",
  "dialog.modelsHint": "Only configured models are shown; gpt-image belongs to GPT, while other image, embedding, and reranker models are not auto-classified",
  "dialog.channel": "Site",
  "dialog.name": "Name",
  "dialog.typeNew2api": "new2api (token balance)",
  "dialog.typeSub2api": "sub2api (login monitors)",
  "dialog.token": "Access token (optional)",
  "dialog.tokenHint": "Leave empty to show model-plaza data only (no balance)",
  "dialog.userId": "User ID (New-Api-User, optional)",
  "dialog.username": "Account (email)",
  "dialog.password": "Password",
  "dialog.vpn": "Via proxy (vpn)",
  "dialog.delete": "Delete site",
  "dialog.cancel": "Cancel",
  "dialog.save": "Save",
  "dialog.saving": "Saving…",
  "dialog.add": "Add",
  "dialog.addModelHint": "Add model name, press Enter",
  "dialog.autoRefresh": "Auto refresh",
  "dialog.off": "Off",
  "dialog.intervalMinutes": "{n} min",
  "dialog.debug": "Debug mode",
  "dialog.debugHint": "Keep raw response data in results",
  "dialog.cardSort": "Card order",
  "dialog.sortAuto": "Auto (by success rate)",
  "dialog.sortManual": "Manual (drag to reorder)",
  "dialog.sortHint": "Drag ⠿ to reorder the site cards",
  "notif.siteFailed": "{name} refresh failed",
  "notif.siteRecovered": "{name} recovered",
  "notif.channelsFailed": "{name} channel failure",
  "notif.andMore": "and {count} more channels",
  "update.available": "New version {version} available",
  "update.download": "Get update",
  "trend.tooltip": "Success rate (24h): {first} → {last} (since {start})",
};

const DICTS: Record<Lang, Record<string, string>> = { zh, en };

export interface I18n {
  lang: Lang;
  setLang: (lang: Lang) => void;
  /** 取词条并做 {var} 插值 */
  t: (key: string, vars?: Record<string, string | number>) => string;
}

const I18nContext = createContext<I18n | null>(null);

function interpolate(template: string, vars?: Record<string, string | number>): string {
  if (!vars) return template;
  return template.replace(/\{(\w+)\}/g, (match, name: string) =>
    name in vars ? String(vars[name]) : match,
  );
}

export function I18nProvider({ children }: { children: ReactNode }) {
  const [lang, setLangState] = useState<Lang>(() => {
    const saved = localStorage.getItem(LANG_KEY);
    return saved === "en" ? "en" : "zh";
  });

  const setLang = (next: Lang) => {
    setLangState(next);
    localStorage.setItem(LANG_KEY, next);
  };

  const value: I18n = {
    lang,
    setLang,
    t: (key, vars) => interpolate(DICTS[lang][key] ?? key, vars),
  };

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18n {
  const ctx = useContext(I18nContext);
  if (!ctx) throw new Error("useI18n 必须在 I18nProvider 内使用");
  return ctx;
}

/** 供非组件代码使用的纯函数版 t（跟随当前 localStorage 语言） */
export function translate(lang: Lang, key: string, vars?: Record<string, string | number>) {
  return interpolate(DICTS[lang][key] ?? key, vars);
}
