import { LOCALES, type Locale } from "../i18n/index";
import { useI18n } from "../i18n/context";

export function LanguageSwitch() {
  const { locale, setLocale, t } = useI18n();

  return (
    <div className="lang-switch" title={t("app.language")}>
      {LOCALES.map((l) => (
        <button
          key={l.id}
          type="button"
          className={"lang-btn" + (locale === l.id ? " active" : "")}
          onClick={() => setLocale(l.id as Locale)}
        >
          {l.label}
        </button>
      ))}
    </div>
  );
}
