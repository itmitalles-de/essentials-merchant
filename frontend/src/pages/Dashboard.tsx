import { useEffect, useState } from "react";
import { useLanguage } from "../contexts/LanguageContext";
import { getHealth } from "../api";

export function Dashboard() {
  const { t } = useLanguage();
  const [apiOk, setApiOk] = useState<boolean | null>(null);

  useEffect(() => {
    getHealth()
      .then(() => setApiOk(true))
      .catch(() => setApiOk(false));
  }, []);

  return (
    <div className="card">
      <h1>{t("dashboard.title")}</h1>
      <p>
        {t("dashboard.apiStatus")}:{" "}
        {apiOk === null ? "…" : apiOk ? t("dashboard.apiStatusOk") : t("dashboard.apiStatusError")}
      </p>
    </div>
  );
}
