import { useState } from "react";
import type { FormEvent } from "react";
import { useNavigate } from "react-router-dom";
import { useAuth } from "../contexts/AuthContext";
import { useLanguage } from "../contexts/LanguageContext";
import { ThemeToggle } from "../components/ThemeToggle";
import { LanguageToggle } from "../components/LanguageToggle";

export function Login() {
  const { login } = useAuth();
  const { t } = useLanguage();
  const navigate = useNavigate();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const onSubmit = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    setSubmitting(true);
    try {
      await login(username, password);
      navigate("/");
    } catch {
      setError(t("login.error"));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        minHeight: "100vh",
        gap: "1rem",
      }}
    >
      <div style={{ position: "absolute", top: 16, right: 16, display: "flex", gap: "0.4rem" }}>
        <ThemeToggle />
        <LanguageToggle />
      </div>
      <form
        onSubmit={onSubmit}
        className="card"
        style={{ width: 320, display: "flex", flexDirection: "column", gap: "0.75rem" }}
      >
        <h2 style={{ margin: 0 }}>{t("app.title")}</h2>
        <input
          placeholder={t("login.username")}
          value={username}
          onChange={(e) => setUsername(e.target.value)}
          autoFocus
        />
        <input
          placeholder={t("login.password")}
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
        />
        {error && <div style={{ color: "var(--danger)" }}>{error}</div>}
        <button type="submit" disabled={submitting}>
          {t("login.submit")}
        </button>
      </form>
    </div>
  );
}
