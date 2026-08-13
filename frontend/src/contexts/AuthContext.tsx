import { createContext, useContext, useEffect, useState } from "react";
import type { ReactNode } from "react";
import { api, getToken, setToken } from "../api";

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
    if (!getToken()) {
      setLoading(false);
      return;
    }
    api
      .get<{ username: string; role: "administrator" | "user" }>("/auth/me")
      .then((me) => {
        setUsername(me.username);
        setRole(me.role);
      })
      .catch(() => setToken(null))
      .finally(() => setLoading(false));
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
