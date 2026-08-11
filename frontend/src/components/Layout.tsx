import type { ReactNode } from "react";
import { useLanguage } from "../contexts/LanguageContext";
import { ThemeToggle } from "./ThemeToggle";
import { LanguageToggle } from "./LanguageToggle";

export function Layout({ children }: { children: ReactNode }) {
  const { t } = useLanguage();
  return (
    <div style={{ maxWidth: 960, margin: "0 auto", padding: "1.5rem" }}>
      <header
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          marginBottom: "1.5rem",
        }}
      >
        <strong>{t("app.title")}</strong>
        <div style={{ display: "flex", gap: "0.5rem" }}>
          <ThemeToggle />
          <LanguageToggle />
        </div>
      </header>
      <main>{children}</main>
    </div>
  );
}
