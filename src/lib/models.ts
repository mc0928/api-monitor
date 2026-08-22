import type { ChannelStatus, MonitorModels, ProviderId } from "../types";

export const PROVIDERS: ProviderId[] = ["gpt", "claude", "grok", "kimi", "gemini", "qwen", "seedream"];

const PROVIDER_ALIASES: Record<string, ProviderId> = {
  openai: "gpt",
  chatgpt: "gpt",
  anthropic: "claude",
  xai: "grok",
  moonshot: "kimi",
  google: "gemini",
  alibaba: "qwen",
  dashscope: "qwen",
  bytedance: "seedream",
  volcengine: "seedream",
};

const copyModels = (m: MonitorModels): MonitorModels => ({
  gpt: [...m.gpt],
  claude: [...m.claude],
  grok: [...m.grok],
  kimi: [...m.kimi],
  gemini: [...m.gemini],
  qwen: [...(m.qwen ?? DEFAULT_MODELS.qwen)],
  seedream: [...(m.seedream ?? DEFAULT_MODELS.seedream)],
});

export const DEFAULT_MODELS: MonitorModels = {
  gpt: ["gpt-5.6-sol", "gpt-5.6-terra"],
  claude: ["claude-sonnet-5", "claude-opus-5"],
  grok: ["grok-4.6"],
  kimi: ["kimi-k3"],
  gemini: ["gemini-2.5-pro", "gemini-2.5-flash"],
  qwen: ["Qwen/Qwen3-Embedding-0.6B"],
  seedream: ["byte-plus-seedream-4-5"],
};

export const PROVIDER_META: Record<
  ProviderId,
  { label: string; vendor: string; color: string }
> = {
  gpt: { label: "GPT", vendor: "openai", color: "#10a37f" },
  claude: { label: "Claude", vendor: "anthropic", color: "#d97757" },
  grok: { label: "Grok", vendor: "xai", color: "#111827" },
  kimi: { label: "Kimi", vendor: "moonshot", color: "#3b82f6" },
  gemini: { label: "Gemini", vendor: "google", color: "#8b5cf6" },
  qwen: { label: "Qwen", vendor: "alibaba", color: "#615ced" },
  seedream: { label: "Seedream", vendor: "bytedance", color: "#ec4899" },
};

export function normalizeModels(models?: MonitorModels | null): MonitorModels {
  const next = copyModels(models ?? DEFAULT_MODELS);
  return PROVIDERS.some((id) => next[id].length > 0) ? next : copyModels(DEFAULT_MODELS);
}

export function availabilityPct(value: number | null | undefined): number | null {
  if (value == null || Number.isNaN(value)) return null;
  if (value < 0) return 0;
  if (value <= 1) return value * 100;
  return Math.min(100, value);
}

export function detectProvider(text: string): ProviderId | null {
  const raw = text.toLowerCase();
  const n = raw.replace(/[./_:]/g, "-");
  if (isNonChatModel(n)) return null;
  if (
    n.includes("claude") ||
    n.includes("sonnet") ||
    n.includes("opus") ||
    n.includes("haiku") ||
    raw.includes("anthropic")
  ) {
    return "claude";
  }
  if (n.includes("grok") || raw.includes("xai")) return "grok";
  if (n.includes("kimi") || n.includes("moonshot")) return "kimi";
  if (n.includes("gemini") || raw.includes("google")) return "gemini";
  if (n.includes("qwen") || raw.includes("dashscope") || raw.includes("alibaba")) return "qwen";
  if (n.includes("seedream") || raw.includes("volcengine") || raw.includes("bytedance")) return "seedream";
  if (
    n.includes("gpt") ||
    n.includes("chatgpt") ||
    n.includes("openai") ||
    /(^|[^a-z])o[134]([^a-z]|$)/.test(n)
  ) {
    return "gpt";
  }
  return null;
}

export function isNonChatModel(text: string): boolean {
  const n = text.toLowerCase().replace(/[./_:]/g, "-");
  return [
    "dall-e",
    "dalle",
    "imagen",
    "nano-banana",
  ].some((marker) => n.includes(marker));
}

export function channelProvider(channel: ChannelStatus): ProviderId | null {
  const id = channel.provider?.toLowerCase();
  if (id) {
    if (PROVIDERS.includes(id as ProviderId)) return id as ProviderId;
    return PROVIDER_ALIASES[id] ?? null;
  }
  return (
    detectProvider(channel.model ?? "") ??
    detectProvider(channel.name) ??
    detectProvider(channel.detail)
  );
}

export function filterChannels(
  channels: ChannelStatus[],
  provider: ProviderId | "all",
): ChannelStatus[] {
  const list =
    provider === "all"
      ? [...channels]
      : channels.filter((ch) => channelProvider(ch) === provider);
  return list.sort((a, b) => {
    const av = (x: ChannelStatus) => availabilityPct(x.availability) ?? -1;
    return av(b) - av(a);
  });
}

export function bestAvailability(channels: ChannelStatus[]): number {
  let best = -1;
  for (const ch of channels) {
    const pct = availabilityPct(ch.availability);
    if (pct != null && pct > best) best = pct;
  }
  return best;
}
