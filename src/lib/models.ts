import type { ChannelStatus, MonitorModels, ProviderId } from "../types";

export const PROVIDERS: ProviderId[] = ["gpt", "claude", "grok", "kimi"];

export const DEFAULT_MODELS: MonitorModels = {
  gpt: ["gpt-5.6-sol", "gpt-5.6-terra"],
  claude: ["claude-sonnet-5", "claude-opus-5"],
  grok: ["grok-4.6"],
  kimi: ["kimi-k3"],
};

export const PROVIDER_META: Record<
  ProviderId,
  { label: string; vendor: string; color: string }
> = {
  gpt: { label: "GPT", vendor: "openai", color: "#10a37f" },
  claude: { label: "Claude", vendor: "anthropic", color: "#d97757" },
  grok: { label: "Grok", vendor: "xai", color: "#111827" },
  kimi: { label: "Kimi", vendor: "moonshot", color: "#3b82f6" },
};

export function normalizeModels(models?: MonitorModels | null): MonitorModels {
  const src = models ?? DEFAULT_MODELS;
  const next: MonitorModels = {
    gpt: [...(src.gpt ?? [])],
    claude: [...(src.claude ?? [])],
    grok: [...(src.grok ?? [])],
    kimi: [...(src.kimi ?? [])],
  };
  const total = PROVIDERS.reduce((n, id) => n + next[id].length, 0);
  return total === 0
    ? {
        gpt: [...DEFAULT_MODELS.gpt],
        claude: [...DEFAULT_MODELS.claude],
        grok: [...DEFAULT_MODELS.grok],
        kimi: [...DEFAULT_MODELS.kimi],
      }
    : next;
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

export function channelProvider(channel: ChannelStatus): ProviderId | null {
  if (channel.provider) {
    const id = channel.provider.toLowerCase();
    if (id === "gpt" || id === "claude" || id === "grok" || id === "kimi") return id;
    if (id === "openai" || id === "chatgpt") return "gpt";
    if (id === "anthropic") return "claude";
    if (id === "xai") return "grok";
    if (id === "moonshot") return "kimi";
  }
  return (
    detectProvider(channel.model ?? "") ||
    detectProvider(channel.name) ||
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
