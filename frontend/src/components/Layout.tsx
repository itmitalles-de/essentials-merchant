import { NavLink, Outlet } from "react-router-dom";
import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { api } from "../api";
import { useAuth } from "../contexts/AuthContext";
import { useLanguage } from "../contexts/LanguageContext";
import { ThemeToggle } from "./ThemeToggle";
import { LanguageToggle } from "./LanguageToggle";
import type { EssentialsModule } from "../types";
import { usePilotStatus } from "../hooks/usePilotStatus";
import { isMantlePilotExperience } from "../pilot";

export function Layout() {
  const { username, role, logout } = useAuth();
  const { t } = useLanguage();
  const [modules, setModules] = useState<EssentialsModule[]>([]);
  const pilot = usePilotStatus();
  const pilotExperience = isMantlePilotExperience();

  useEffect(() => {
    api.get<EssentialsModule[]>("/modules").then(setModules).catch(() => setModules([]));
  }, []);
  const enabled = (moduleId: string) => modules.some((module) => module.module_id === moduleId && module.enabled);

  return (
    <div style={{ display: "flex", minHeight: "100vh" }}>
      <nav
        style={{
          width: 200,
          borderRight: "1px solid var(--border)",
          padding: "1rem",
          display: "flex",
          flexDirection: "column",
          gap: "0.4rem",
        }}
      >
        <div className={pilotExperience ? "pilot-brand" : undefined} style={{ fontWeight: 700, marginBottom: "0.8rem" }}>
          {pilotExperience && <img src="/ai-marketing-icon.svg" alt="" width="34" height="34" />}
          <span>{pilotExperience ? "Mantle · AI Marketing" : "Essentials+ Merchant"}</span>
        </div>
        {pilotExperience ? (
          <NavItem to="/ai-marketing">Amazon Analyse</NavItem>
        ) : (
          <>
            <NavItem to="/">{t("nav.dashboard")}</NavItem>
            {!pilot?.enabled && enabled("core.orders") && <NavItem to="/customers">{t("nav.customers")}</NavItem>}
            {!pilot?.enabled && enabled("accounting.invoices") && <NavItem to="/invoices">{t("nav.invoices")}</NavItem>}
            {!pilot?.enabled && enabled("core.catalog") && <NavItem to="/articles">{t("nav.articles")}</NavItem>}
            {!pilot?.enabled && enabled("core.orders") && <NavItem to="/sales-orders">{t("nav.salesOrders")}</NavItem>}
            {!pilot?.enabled && enabled("core.catalog") && <NavItem to="/settings">{t("nav.settings")}</NavItem>}
            {enabled("marketplace.amazon_intelligence") && <NavItem to="/marketplace">Marketplace Intelligence</NavItem>}
            {role === "administrator" && <NavItem to="/admin-center">Admin-Center</NavItem>}
            {role === "administrator" && enabled("commerce.vendure") && <NavItem to="/integration-diagnostics">Integrationsdiagnose</NavItem>}
          </>
        )}
        <div style={{ flex: 1 }} />
        {!pilotExperience && <div style={{ fontSize: "0.85rem", color: "var(--fg-muted)" }}>{username}</div>}
        <ThemeToggle />
        <LanguageToggle />
        {!pilotExperience && (
          <button className="secondary" onClick={logout}>
            {t("nav.logout")}
          </button>
        )}
      </nav>
      <main style={{ flex: 1, padding: "1.5rem" }}>
        {pilot?.enabled && (
          <div className="card" role="status" data-testid="pilot-banner" style={{ borderColor: pilot.compliant ? "var(--accent)" : "var(--danger)", marginBottom: "1rem" }}>
            <strong>{pilot.title}</strong>
            <div style={{ color: "var(--fg-muted)", marginTop: "0.25rem" }}>
              {pilot.compliant ? "Fail-closed Pilotprofil aktiv" : "Pilotprofil ist nicht konform – keine Live-Anforderung ausführen"}
            </div>
          </div>
        )}
        <Outlet />
      </main>
    </div>
  );
}

function NavItem({ to, children }: { to: string; children: ReactNode }) {
  return (
    <NavLink
      to={to}
      end={to === "/"}
      style={({ isActive }) => ({
        padding: "0.5rem 0.6rem",
        borderRadius: 6,
        textDecoration: "none",
        color: isActive ? "var(--accent-fg)" : "var(--fg)",
        background: isActive ? "var(--accent)" : "transparent",
        fontWeight: isActive ? 600 : 400,
      })}
    >
      {children}
    </NavLink>
  );
}
