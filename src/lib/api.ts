import { invoke } from "@tauri-apps/api/core";
import type { AppConfig, SiteConfig, SiteResult } from "../types";

export const getConfig = () => invoke<AppConfig>("get_config");

export const saveConfig = (cfg: AppConfig) => invoke<void>("save_config", { cfg });

export const refreshSite = (id: string) => invoke<SiteResult>("refresh_site", { id });

export const refreshAll = () => invoke<SiteResult[]>("refresh_all");

export const getResults = () => invoke<SiteResult[]>("get_results");

export const testProxy = () => invoke<string>("test_proxy");

/** 打开内嵌浏览器登录窗口（sub2api）：窗口内完成登录/人机验证后令牌自动回传 */
export const openWebLogin = (site: SiteConfig) => invoke<void>("open_web_login", { site });
