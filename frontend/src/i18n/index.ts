import { en, type Messages } from "./locales/en";
import { zh } from "./locales/zh";

export type Locale = "en" | "zh";

export const LOCALES: { id: Locale; label: string }[] = [
  { id: "en", label: "English" },
  { id: "zh", label: "中文" },
];

export const locales: Record<Locale, Messages> = { en, zh };

const STORAGE_KEY = "hl_locale";

function detectLocale(): Locale {
  const saved = localStorage.getItem(STORAGE_KEY);
  if (saved === "en" || saved === "zh") return saved;
  const lang = navigator.language.toLowerCase();
  if (lang.startsWith("zh")) return "zh";
  return "en";
}

let currentLocale: Locale = detectLocale();
let dict: Messages = locales[currentLocale];
const listeners = new Set<() => void>();

function getPath(obj: Record<string, unknown>, path: string): unknown {
  return path.split(".").reduce<unknown>((o, k) => {
    if (o && typeof o === "object" && k in (o as Record<string, unknown>)) {
      return (o as Record<string, unknown>)[k];
    }
    return undefined;
  }, obj);
}

/** Translate a dot-path key with optional `{var}` interpolation. Works outside React. */
export function t(key: string, params?: Record<string, string | number>): string {
  const raw = getPath(dict as unknown as Record<string, unknown>, key);
  let s = typeof raw === "string" ? raw : key;
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      s = s.replace(new RegExp(`\\{${k}\\}`, "g"), String(v));
    }
  }
  return s;
}

export function getLocale(): Locale {
  return currentLocale;
}

export function setLocale(locale: Locale): void {
  if (locale === currentLocale) return;
  currentLocale = locale;
  dict = locales[locale];
  localStorage.setItem(STORAGE_KEY, locale);
  document.documentElement.lang = locale === "zh" ? "zh-CN" : "en";
  listeners.forEach((fn) => fn());
}

export function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Call once at startup (before first render). */
export function initI18n(): void {
  document.documentElement.lang = currentLocale === "zh" ? "zh-CN" : "en";
}

export type { Messages };
