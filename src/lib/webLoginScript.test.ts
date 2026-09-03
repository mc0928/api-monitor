// WEB_LOGIN_SCRIPT（src-tauri/src/lib.rs 内嵌字符串）的行为回归测试。
// 脚本无类型保护且逻辑敏感：曾因 webview localStorage 残留过期令牌，
// SPA 带旧 Bearer 请求公开接口（恒 200）导致登录窗口刚打开就被误关（“闪退”）。
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { beforeEach, describe, expect, it, vi } from "vitest";
import vm from "node:vm";

function loadScript(): string {
  const lib = readFileSync(resolve(__dirname, "../../src-tauri/src/lib.rs"), "utf-8");
  const start = lib.indexOf('r#"(function');
  const end = lib.indexOf('"#;', start);
  const js = lib.slice(start + 3, end);
  if (!js.startsWith("(function") || !js.endsWith("})();")) {
    throw new Error("未能从 lib.rs 提取 WEB_LOGIN_SCRIPT（内嵌脚本位置或格式有变）");
  }
  return js;
}

const b64url = (obj: unknown) =>
  btoa(JSON.stringify(obj)).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
const validJwt = `eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.${b64url({
  user_id: 1,
  exp: Math.floor(Date.now() / 1000) + 86400,
})}.sig`;
const expiredJwt = `eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.${b64url({
  user_id: 260,
  exp: 1_000_000_000,
  iat: 999_999_999,
})}.sig`;

interface Env {
  navigated: string | null;
  handlers: Record<string, { status: number; body: string }>;
  window: Record<string, unknown>;
  xhrClass: new () => MockXHR;
  token(): { auth_token: string; refresh_token: string | null } | null;
}

class MockXHR {
  listeners: Record<string, Array<() => void>> = {};
  responseURL = "";
  status = 0;
  responseText = "";
  env!: Env;
  setRequestHeader(_n: string, _v: string) {}
  addEventListener(ev: string, fn: () => void) {
    (this.listeners[ev] ??= []).push(fn);
  }
  getResponseHeader(k: string) {
    return k.toLowerCase() === "content-type" ? "application/json" : null;
  }
  open(_m: string, url: string) {
    this.responseURL = url;
  }
  send() {
    const handler = this.env.handlers[this.responseURL];
    this.status = handler ? handler.status : 404;
    this.responseText = handler ? handler.body : "{}";
    for (const fn of this.listeners.load ?? []) fn();
  }
}

function makeEnv({ topEqualsSelf = true } = {}): Env {
  const env: Env = {
    navigated: null,
    handlers: {},
    window: {},
    xhrClass: MockXHR,
    token() {
      if (!this.navigated?.startsWith("http://apimon-token.local/#")) return null;
      return JSON.parse(decodeURIComponent(this.navigated.split("#")[1]));
    },
  };
  const window: Record<string, unknown> = {};
  Object.defineProperty(window, "location", {
    get: () => ({
      set href(v: string) {
        env.navigated = v;
      },
    }),
  });
  window.top = topEqualsSelf ? window : { other: true };
  window.fetch = async (input: string | { url: string }, _init?: { headers?: unknown }) => {
    const url = typeof input === "string" ? input : input.url;
    const handler = env.handlers[url];
    return {
      status: handler ? handler.status : 404,
      headers: { get: (k: string) => (k.toLowerCase() === "content-type" ? "application/json" : null) },
      clone() {
        return this;
      },
      text: async () => (handler ? handler.body : "{}"),
    };
  };
  env.window = window;
  const context = vm.createContext({
    window,
    location: window.location,
    fetch: window.fetch,
    XMLHttpRequest: MockXHR,
    atob: (s: string) => atob(s),
  });
  // MockXHR.send 需要回指 env；挂到原型上供所有实例使用
  const proto = MockXHR.prototype as unknown as Record<string, unknown>;
  proto.env = env;
  vm.runInContext(loadScript(), context);
  return env;
}

const fetch = (env: Env, url: string, init?: { headers?: unknown; method?: string }) =>
  (env.window.fetch as (u: string, i?: { headers?: unknown; method?: string }) => Promise<unknown>)(
    url,
    init,
  );

const xhrLogin = (env: Env, url = "/api/v1/auth/login") => {
  const xhr = new env.xhrClass();
  (xhr as unknown as { env: Env }).env = env;
  xhr.open("POST", url);
  xhr.send();
};

beforeEach(() => {
  vi.restoreAllMocks();
});

describe("WEB_LOGIN_SCRIPT 令牌捕获", () => {
  it("过期会话请求鉴权接口返回 401：不触发（旧版此处导致闪退）", async () => {
    const env = makeEnv();
    env.handlers["/api/v1/auth/me"] = { status: 401, body: '{"code":401}' };
    await fetch(env, "/api/v1/auth/me", { headers: { Authorization: `Bearer ${expiredJwt}` } });
    expect(env.token()).toBeNull();
  });

  it("过期 JWT 请求公开接口（恒 200）：不触发（原始闪退路径）", async () => {
    const env = makeEnv();
    env.handlers["/api/v1/settings/public?timezone=Asia%2FShanghai"] = {
      status: 200,
      body: '{"data":{"turnstile_enabled":true}}',
    };
    await fetch(env, "/api/v1/settings/public?timezone=Asia%2FShanghai", {
      headers: { Authorization: `Bearer ${expiredJwt}` },
    });
    expect(env.token()).toBeNull();
  });

  it("有效 JWT 带 Bearer 且响应 200：触发捕获", async () => {
    const env = makeEnv();
    env.handlers["/api/v1/channel-monitors"] = { status: 200, body: '{"data":[]}' };
    await fetch(env, "/api/v1/channel-monitors", {
      headers: { Authorization: `Bearer ${validJwt}` },
    });
    expect(env.token()).toEqual({ auth_token: validJwt, refresh_token: null });
  });

  it("非认证接口响应里的 token 字段：不触发", async () => {
    const env = makeEnv();
    env.handlers["/api/v1/settings/public"] = {
      status: 200,
      body: '{"data":{"token":"abcdefghijklmnopq"}}',
    };
    await fetch(env, "/api/v1/settings/public");
    expect(env.token()).toBeNull();
  });

  it("XHR 登录响应：触发并带 refresh_token", () => {
    const env = makeEnv();
    env.handlers["/api/v1/auth/login"] = {
      status: 200,
      body: `{"data":{"access_token":"${validJwt}","refresh_token":"rt_1234567890abcdef"}}`,
    };
    xhrLogin(env);
    expect(env.token()).toEqual({
      auth_token: validJwt,
      refresh_token: "rt_1234567890abcdef",
    });
  });

  it("登录响应返回过期 JWT：不触发", () => {
    const env = makeEnv();
    env.handlers["/api/v1/auth/login"] = {
      status: 200,
      body: `{"data":{"access_token":"${expiredJwt}"}}`,
    };
    xhrLogin(env);
    expect(env.token()).toBeNull();
  });

  it("iframe 内不劫持（不破坏 Turnstile 验证组件）", () => {
    const env = makeEnv({ topEqualsSelf: false });
    env.handlers["/api/v1/auth/login"] = {
      status: 200,
      body: `{"data":{"access_token":"${validJwt}"}}`,
    };
    xhrLogin(env);
    expect(env.token()).toBeNull();
  });

  it("fetch 登录响应（data 包裹 auth_token）：触发", async () => {
    const env = makeEnv();
    env.handlers["/api/v1/auth/login"] = {
      status: 200,
      body: `{"data":{"auth_token":"${validJwt}"}}`,
    };
    await fetch(env, "/api/v1/auth/login", { method: "POST" });
    expect(env.token()).toEqual({ auth_token: validJwt, refresh_token: null });
  });

  it("OAuth 回调路径的登录响应：触发（覆盖第三方登录）", async () => {
    const env = makeEnv();
    env.handlers["/api/v1/auth/linuxdo/callback?code=1"] = {
      status: 200,
      body: `{"data":{"access_token":"${validJwt}"}}`,
    };
    await fetch(env, "/api/v1/auth/linuxdo/callback?code=1");
    expect(env.token()).not.toBeNull();
  });

  it("非 JWT 不透明令牌带 Bearer 且 200：仍触发（无法本地判断过期）", async () => {
    const env = makeEnv();
    env.handlers["/api/v1/auth/me"] = { status: 200, body: '{"data":{"user_id":1}}' };
    await fetch(env, "/api/v1/auth/me", { headers: { Authorization: "Bearer " + "y".repeat(40) } });
    expect(env.token()).not.toBeNull();
  });
});
