import { useLanguage } from "../contexts/LanguageContext";

export function LanguageToggle() {
  const { choice, setChoice, t } = useLanguage();
  return (
    <select
      value={choice}
      onChange={(e) => setChoice(e.target.value as "system" | "en" | "de")}
      aria-label={t("common.language")}
      title={t("common.language")}
    >
      <option value="system">{t("common.language.system")}</option>
      <option value="en">{t("common.language.en")}</option>
      <option value="de">{t("common.language.de")}</option>
    </select>
  );
}
