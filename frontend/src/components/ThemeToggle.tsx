import { useEffect, useState } from "react";

import { useTheme } from "../contexts/ThemeContext";
import { useLanguage } from "../contexts/LanguageContext";

function systemPrefersDark(): boolean {
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false;
}

export function ThemeToggle({ variant = "select" }: { variant?: "select" | "switch" }) {
  const { choice, setChoice } = useTheme();
  const { t } = useLanguage();
  const [systemDark, setSystemDark] = useState(systemPrefersDark);

  useEffect(() => {
    const query = window.matchMedia("(prefers-color-scheme: dark)");
    const update = () => setSystemDark(query.matches);
    query.addEventListener("change", update);
    return () => query.removeEventListener("change", update);
  }, []);

  if (variant === "switch") {
    const dark = choice === "dark" || (choice === "system" && systemDark);
    const target = dark ? "light" : "dark";
    return (
      <button
        type="button"
        className="theme-switch"
        data-testid="theme-switch"
        role="switch"
        aria-checked={dark}
        aria-label={`Darstellung: ${dark ? "Dunkel" : "Hell"}. Zu ${target === "dark" ? "Dunkel" : "Hell"} wechseln`}
        title={`Zu ${target === "dark" ? "Dunkel" : "Hell"} wechseln`}
        onClick={() => setChoice(target)}
      >
        <span className="theme-switch-track" aria-hidden="true">
          <span className="theme-switch-sun">☀</span>
          <span className="theme-switch-moon">☾</span>
          <span className="theme-switch-thumb" />
        </span>
        <span>{dark ? "Dunkel" : "Hell"}</span>
      </button>
    );
  }

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
