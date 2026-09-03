import { useSyncExternalStore } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { errMsg } from "./errors";

/** 记录用户已关闭横幅的版本号（同 cc-switch：只关闭该版本的提醒） */
const DISMISSED_VERSION_KEY = "api-monitor:update:dismissedVersion";

export interface UpdaterState {
  /** 当前应用版本（首次获取前为 null） */
  currentVersion: string | null;
  /** 可升级到的版本号（如 v0.2.1）；无更新或未检查时为 null */
  availableVersion: string | null;
  checking: boolean;
  downloading: boolean;
  /** 下载进度百分比；响应不含总大小时为 null */
  progress: number | null;
  /** 手动检查后的反馈：null=未检查，true=已是最新，false=有新版本 */
  upToDate: boolean | null;
  /** 最近的检查/安装错误文案（启动静默检查失败不写入） */
  error: string | null;
  /** 当前可用版本已被用户关闭横幅 */
  dismissed: boolean;
}

let state: UpdaterState = {
  currentVersion: null,
  availableVersion: null,
  checking: false,
  downloading: false,
  progress: null,
  upToDate: null,
  error: null,
  dismissed: false,
};

const listeners = new Set<() => void>();

function setState(patch: Partial<UpdaterState>) {
  state = { ...state, ...patch };
  for (const notify of listeners) notify();
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** check() 返回的更新对象只在本次会话内有效，安装时直接复用 */
let pendingUpdate: Update | null = null;
let checking = false;
let downloading = false;

/** 检查更新；返回是否存在新版本。silent=true 供启动时调用，失败静默 */
export function checkForUpdate(opts: { silent?: boolean } = {}): Promise<boolean> {
  if (checking || downloading) return Promise.resolve(pendingUpdate != null);
  checking = true;
  setState({ checking: true, error: null, upToDate: null });
  return (async () => {
    try {
      const update = await check();
      pendingUpdate = update ?? null;
      if (update) {
        setState({
          checking: false,
          availableVersion: update.version,
          upToDate: false,
          dismissed: localStorage.getItem(DISMISSED_VERSION_KEY) === update.version,
        });
        return true;
      }
      setState({ checking: false, availableVersion: null, upToDate: true, dismissed: false });
      return false;
    } catch (e) {
      setState({ checking: false, error: opts.silent ? null : errMsg(e) });
      return false;
    } finally {
      checking = false;
    }
  })();
}

/** 下载并安装更新，成功后自动重启（同 cc-switch 的 installUpdateAndRestart） */
export async function installUpdate(): Promise<void> {
  if (downloading) return;
  if (!pendingUpdate) {
    // 会话内尚未检查过（如重启后直接点更新）：先检查一次
    await checkForUpdate();
    if (!pendingUpdate) return;
  }
  downloading = true;
  setState({ downloading: true, progress: null, error: null });
  const update = pendingUpdate;
  let received = 0;
  let contentLength: number | null = null;
  try {
    await update.downloadAndInstall((event) => {
      if (event.event === "Started" && event.data.contentLength != null) {
        contentLength = event.data.contentLength;
      } else if (event.event === "Progress") {
        received += event.data.chunkLength;
        setState({
          progress:
            contentLength != null && contentLength > 0
              ? Math.min(100, Math.round((received / contentLength) * 100))
              : null,
        });
      }
    });
    await relaunch();
  } catch (e) {
    setState({ downloading: false, error: errMsg(e) });
  } finally {
    downloading = false;
  }
}

export function dismissUpdate() {
  if (state.availableVersion) {
    localStorage.setItem(DISMISSED_VERSION_KEY, state.availableVersion);
  }
  setState({ dismissed: true });
}

/** 当前应用版本：模块加载时异步取一次，供设置页展示 */
void getVersion()
  .then((version) => setState({ currentVersion: version }))
  .catch(() => {});

/** 供 App 横幅与设置页共享的更新状态（模块级单例 store） */
export function useUpdater(): UpdaterState {
  return useSyncExternalStore(subscribe, () => state);
}
