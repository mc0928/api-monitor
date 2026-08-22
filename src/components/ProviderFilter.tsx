import { PROVIDER_META, PROVIDERS } from "../lib/models";
import { useI18n } from "../lib/i18n";
import type { ProviderId } from "../types";
import { ProviderIcon } from "./ProviderIcons";

interface Props {
  value: ProviderId | "all";
  onChange: (value: ProviderId | "all") => void;
}

export default function ProviderFilter({ value, onChange }: Props) {
  const { t } = useI18n();
  const chip =
    "inline-flex items-center gap-1.5 rounded-full border px-3 py-1 text-sm transition";
  return (
    <div className="flex flex-wrap items-center gap-2">
      <button
        type="button"
        onClick={() => onChange("all")}
        className={`${chip} ${
          value === "all"
            ? "border-gray-900 bg-gray-900 text-white dark:border-gray-100 dark:bg-gray-100 dark:text-gray-900"
            : "border-gray-300 bg-white text-gray-600 hover:bg-gray-50 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-300 dark:hover:bg-gray-800"
        }`}
      >
        {t("filter.all")}
      </button>
      {PROVIDERS.map((id) => {
        const active = value === id;
        return (
          <button
            key={id}
            type="button"
            onClick={() => onChange(id)}
            className={`${chip} ${
              active
                ? "border-gray-900 bg-white text-gray-900 shadow-sm dark:border-gray-100 dark:bg-gray-800 dark:text-gray-100"
                : "border-gray-300 bg-white text-gray-600 hover:bg-gray-50 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-300 dark:hover:bg-gray-800"
            }`}
          >
            <ProviderIcon provider={id} size={14} />
            <span>{PROVIDER_META[id].label}</span>
          </button>
        );
      })}
    </div>
  );
}
