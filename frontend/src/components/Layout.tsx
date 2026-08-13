import { NavLink, Outlet } from "react-router-dom";
import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { api } from "../api";
import { useAuth } from "../contexts/AuthContext";
import { useLanguage } from "../contexts/LanguageContext";
import { ThemeToggle } from "./ThemeToggle";
import { LanguageToggle } from "./LanguageToggle";
import type { EssentialsModule } from "../types";

export function Layout() {
  const { username, role, logout } = useAuth();
  const { t } = useLanguage();
  const [modules, setModules] = useState<EssentialsModule[]>([]);

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
        <div style={{ fontWeight: 700, marginBottom: "0.8rem" }}>Essentials+ Merchant</div>
        <NavItem to="/">{t("nav.dashboard")}</NavItem>
        {enabled("core.orders") && <NavItem to="/customers">{t("nav.customers")}</NavItem>}
        {enabled("accounting.invoices") && <NavItem to="/invoices">{t("nav.invoices")}</NavItem>}
        {enabled("core.catalog") && <NavItem to="/articles">{t("nav.articles")}</NavItem>}
        {enabled("core.orders") && <NavItem to="/sales-orders">{t("nav.salesOrders")}</NavItem>}
        {enabled("core.catalog") && <NavItem to="/settings">{t("nav.settings")}</NavItem>}
        {enabled("marketplace.amazon_intelligence") && <NavItem to="/marketplace">Marketplace Intelligence</NavItem>}
        {role === "administrator" && <NavItem to="/admin-center">Admin-Center</NavItem>}
        {role === "administrator" && enabled("commerce.vendure") && <NavItem to="/integration-diagnostics">Integrationsdiagnose</NavItem>}
        <div style={{ flex: 1 }} />
        <div style={{ fontSize: "0.85rem", color: "var(--fg-muted)" }}>{username}</div>
        <ThemeToggle />
        <LanguageToggle />
        <button className="secondary" onClick={logout}>
          {t("nav.logout")}
        </button>
      </nav>
      <main style={{ flex: 1, padding: "1.5rem" }}>
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
