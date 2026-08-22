import { createContext, useContext, useEffect, useState } from "react";
import type { ReactNode } from "react";
import { api, getToken, setToken } from "../api";
import { isMantlePilotExperience } from "../pilot";

interface AuthContextValue {
  isAuthenticated: boolean;
  username: string | null;
  role: "administrator" | "user" | null;
  loading: boolean;
  login: (username: string, password: string) => Promise<void>;
  logout: () => void;
}

const AuthContext = createContext<AuthContextValue | null>(null);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [username, setUsername] = useState<string | null>(null);
  const [role, setRole] = useState<"administrator" | "user" | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let active = true;
    const loadMe = async () => {
      const me = await api.get<{ username: string; role: "administrator" | "user" }>("/auth/me");
      if (active) {
        setUsername(me.username);
        setRole(me.role);
      }
    };
    const bootstrap = async () => {
      try {
        // The public-facing Mantle pilot shell must always replace any token
        // left by another Merchant route with the dedicated read-only scope.
        if (isMantlePilotExperience()) {
          setToken(null);
          const session = await api.post<{ access_token: string }>("/auth/pilot-session");
          setToken(session.access_token);
          await loadMe();
          return;
        }
        if (getToken()) {
          try {
            await loadMe();
            return;
          } catch {
            setToken(null);
          }
        }
      } catch {
        setToken(null);
      } finally {
        if (active) setLoading(false);
      }
    };
    void bootstrap();
    return () => {
      active = false;
    };
  }, []);

  const login = async (u: string, password: string) => {
    const res = await api.post<{ access_token: string }>("/auth/login", {
      username: u,
      password,
    });
    setToken(res.access_token);
    const me = await api.get<{ username: string; role: "administrator" | "user" }>("/auth/me");
    setUsername(me.username);
    setRole(me.role);
  };

  const logout = () => {
    setToken(null);
    setUsername(null);
    setRole(null);
  };

  return (
    <AuthContext.Provider
      value={{ isAuthenticated: !!username, username, role, loading, login, logout }}
    >
      {children}
    </AuthContext.Provider>
  );
}

export function useAuth() {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error("useAuth must be used within AuthProvider");
  return ctx;
}
