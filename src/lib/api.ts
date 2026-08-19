import { invoke } from "@tauri-apps/api/core";
import type { AppConfig, SiteResult } from "../types";

export const getConfig = () => invoke<AppConfig>("get_config");

export const saveConfig = (cfg: AppConfig) => invoke<void>("save_config", { cfg });

export const refreshSite = (id: string) => invoke<SiteResult>("refresh_site", { id });

export const refreshAll = () => invoke<SiteResult[]>("refresh_all");

export const getResults = () => invoke<SiteResult[]>("get_results");

export const testProxy = () => invoke<string>("test_proxy");
