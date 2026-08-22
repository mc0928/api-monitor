import { describe, expect, it } from "vitest";
import {
  availabilityPct,
  bestAvailability,
  channelProvider,
  DEFAULT_MODELS,
  detectProvider,
  filterChannels,
  normalizeModels,
} from "./models";
import type { ChannelStatus } from "../types";

function channel(patch: Partial<ChannelStatus>): ChannelStatus {
  return {
    name: "ch",
    online: true,
    detail: "",
    status: "operational",
    plan_level: null,
    availability: null,
    latency_ms: null,
    tiers: [],
    balances: [],
    ...patch,
  };
}

describe("detectProvider", () => {
  it("识别四个模型族", () => {
    expect(detectProvider("gpt-5.6-sol")).toBe("gpt");
    expect(detectProvider("anthropic/claude-sonnet-4.6")).toBe("claude");
    expect(detectProvider("grok-4.6")).toBe("grok");
    expect(detectProvider("kimi-k3")).toBe("kimi");
  });

  it("识别厂商别名与系列词", () => {
    expect(detectProvider("openai")).toBe("gpt");
    expect(detectProvider("chatgpt-4o")).toBe("gpt");
    expect(detectProvider("anthropic")).toBe("claude");
    expect(detectProvider("claude-opus-5")).toBe("claude");
    expect(detectProvider("xai")).toBe("grok");
    expect(detectProvider("moonshot")).toBe("kimi");
  });

  it("o 系列按 GPT 归类", () => {
    expect(detectProvider("o3-mini")).toBe("gpt");
    expect(detectProvider("o1")).toBe("gpt");
  });

  it("非模型文本返回 null", () => {
    expect(detectProvider("deepseek-v3")).toBeNull();
    expect(detectProvider("random relay group")).toBeNull();
  });
});

describe("availabilityPct", () => {
  it("null / NaN 返回 null", () => {
    expect(availabilityPct(null)).toBeNull();
    expect(availabilityPct(undefined)).toBeNull();
    expect(availabilityPct(Number.NaN)).toBeNull();
  });

  it("边界钳制", () => {
    expect(availabilityPct(-1)).toBe(0);
    expect(availabilityPct(150)).toBe(100);
  });

  it("0~1 视为比率，>1 视为百分数", () => {
    expect(availabilityPct(0.365)).toBeCloseTo(36.5);
    expect(availabilityPct(1)).toBe(100);
    expect(availabilityPct(36.5)).toBeCloseTo(36.5);
  });
});

describe("channelProvider", () => {
  it("provider 字段直用并映射别名", () => {
    expect(channelProvider(channel({ provider: "gpt" }))).toBe("gpt");
    expect(channelProvider(channel({ provider: "anthropic" }))).toBe("claude");
    expect(channelProvider(channel({ provider: "xai" }))).toBe("grok");
    expect(channelProvider(channel({ provider: "moonshot" }))).toBe("kimi");
  });

  it("无 provider 时按 model -> name -> detail 回退", () => {
    expect(channelProvider(channel({ model: "claude-sonnet-5" }))).toBe("claude");
    expect(channelProvider(channel({ name: "Kimi 专用组" }))).toBe("kimi");
    expect(channelProvider(channel({ detail: "gpt-5.6-sol · 100ms" }))).toBe("gpt");
  });
});

describe("filterChannels", () => {
  const channels = [
    channel({ name: "a", model: "gpt-5.6-sol", availability: 88 }),
    channel({ name: "b", model: "claude-sonnet-5", availability: 99.5 }),
    channel({ name: "c", model: "kimi-k3", availability: null }),
  ];

  it("all 返回全部并按成功率降序（null 垫底）", () => {
    const list = filterChannels(channels, "all");
    expect(list.map((c) => c.name)).toEqual(["b", "a", "c"]);
  });

  it("按 provider 过滤", () => {
    const list = filterChannels(channels, "claude");
    expect(list.map((c) => c.name)).toEqual(["b"]);
  });
});

describe("bestAvailability", () => {
  it("取最大成功率", () => {
    expect(
      bestAvailability([channel({ availability: 0.9 }), channel({ availability: 95 })]),
    ).toBe(95);
  });

  it("空列表返回 -1", () => {
    expect(bestAvailability([])).toBe(-1);
  });
});

describe("normalizeModels", () => {
  it("全空回退默认配置", () => {
    const next = normalizeModels({ gpt: [], claude: [], grok: [], kimi: [] });
    expect(next).toEqual(DEFAULT_MODELS);
  });

  it("非空配置整体保留（不回退默认）", () => {
    const next = normalizeModels({ gpt: ["gpt-x"], claude: [], grok: [], kimi: [] });
    expect(next.gpt).toEqual(["gpt-x"]);
    expect(next.claude).toEqual([]);
  });
});
