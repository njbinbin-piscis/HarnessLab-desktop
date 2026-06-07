import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { getLocale, setLocale, subscribe, t, type Locale } from "./index";

type I18nContextValue = {
  locale: Locale;
  setLocale: (locale: Locale) => void;
  t: typeof t;
};

const I18nContext = createContext<I18nContextValue | null>(null);

export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState(getLocale);

  useEffect(() => subscribe(() => setLocaleState(getLocale())), []);

  const value: I18nContextValue = {
    locale,
    setLocale,
    t,
  };

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18nContextValue {
  const ctx = useContext(I18nContext);
  if (!ctx) throw new Error("useI18n must be used within I18nProvider");
  return ctx;
}

/** Shorthand hook — returns only the translate function. */
export function useT() {
  return useI18n().t;
}
