import { NavLink, Outlet } from "react-router-dom";
import type { ReactNode } from "react";
import { useAuth } from "../contexts/AuthContext";
import { useLanguage } from "../contexts/LanguageContext";
import { ThemeToggle } from "./ThemeToggle";
import { LanguageToggle } from "./LanguageToggle";

export function Layout() {
  const { username, logout } = useAuth();
  const { t } = useLanguage();

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
        <div style={{ fontWeight: 700, marginBottom: "1rem" }}>{t("app.title")}</div>
        <NavItem to="/">{t("nav.dashboard")}</NavItem>
        <NavItem to="/customers">{t("nav.customers")}</NavItem>
        <NavItem to="/settings">{t("nav.settings")}</NavItem>
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
