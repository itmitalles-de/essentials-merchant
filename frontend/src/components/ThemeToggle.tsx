import { useTheme } from "../contexts/ThemeContext";
import { useLanguage } from "../contexts/LanguageContext";

export function ThemeToggle() {
  const { choice, setChoice } = useTheme();
  const { t } = useLanguage();
  return (
    <select
      value={choice}
      onChange={(e) => setChoice(e.target.value as "system" | "light" | "dark")}
      aria-label={t("common.colorScheme")}
      title={t("common.colorScheme")}
    >
      <option value="system">{t("common.colorScheme.system")}</option>
      <option value="light">{t("common.colorScheme.light")}</option>
      <option value="dark">{t("common.colorScheme.dark")}</option>
    </select>
  );
}
