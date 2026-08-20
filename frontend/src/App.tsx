import type { ReactNode } from "react";
import { Navigate, Route, Routes } from "react-router-dom";
import { useAuth } from "./contexts/AuthContext";
import { Layout } from "./components/Layout";
import { Login } from "./pages/Login";
import { Dashboard } from "./pages/Dashboard";
import { Customers } from "./pages/Customers";
import { Invoices } from "./pages/Invoices";
import { InvoiceDetail } from "./pages/InvoiceDetail";
import { ArticleDetail } from "./pages/ArticleDetail";
import { Articles } from "./pages/Articles";
import { SalesOrders } from "./pages/SalesOrders";
import { SalesOrderDetail } from "./pages/SalesOrderDetail";
import { Settings } from "./pages/Settings";
import { AdminCenter } from "./pages/AdminCenter";
import { MarketplaceIntelligence } from "./pages/MarketplaceIntelligence";
import { IntegrationDiagnostics } from "./pages/IntegrationDiagnostics";
import { isMantlePilotExperience } from "./pilot";

function RequireAuth({ children }: { children: ReactNode }) {
  const { isAuthenticated, loading } = useAuth();
  if (loading) return null;
  if (!isAuthenticated) {
    return isMantlePilotExperience()
      ? (
        <main className="pilot-unavailable" role="alert">
          <h1>Amazon AI Marketing ist gerade nicht erreichbar</h1>
          <p>Die interne, anonyme Pilotsitzung konnte nicht aufgebaut werden.</p>
        </main>
      )
      : <Navigate to="/login" replace />;
  }
  return <>{children}</>;
}

function HomeRoute() {
  return window.location.hostname === "ai-marketing.mantle-climbing.de"
    ? <Navigate to="/ai-marketing" replace />
    : <Dashboard />;
}

export default function App() {
  const pilotExperience = isMantlePilotExperience();
  return (
    <Routes>
      <Route
        path="/login"
        element={pilotExperience ? <Navigate to="/ai-marketing" replace /> : <Login />}
      />
      <Route
        path="/"
        element={
          <RequireAuth>
            <Layout />
          </RequireAuth>
        }
      >
        <Route index element={<HomeRoute />} />
        {!pilotExperience && <Route path="customers" element={<Customers />} />}
        {!pilotExperience && <Route path="invoices" element={<Invoices />} />}
        {!pilotExperience && <Route path="invoices/:id" element={<InvoiceDetail />} />}
        {!pilotExperience && <Route path="articles" element={<Articles />} />}
        {!pilotExperience && <Route path="articles/:id" element={<ArticleDetail />} />}
        {!pilotExperience && <Route path="sales-orders" element={<SalesOrders />} />}
        {!pilotExperience && <Route path="sales-orders/:id" element={<SalesOrderDetail />} />}
        {!pilotExperience && <Route path="settings" element={<Settings />} />}
        {!pilotExperience && <Route path="admin-center" element={<AdminCenter />} />}
        {!pilotExperience && <Route path="integration-diagnostics" element={<IntegrationDiagnostics />} />}
        <Route
          path="marketplace"
          element={pilotExperience
            ? <Navigate to="/ai-marketing" replace />
            : <MarketplaceIntelligence />}
        />
        <Route path="ai-marketing" element={<MarketplaceIntelligence aiFirst />} />
        {pilotExperience && <Route path="*" element={<Navigate to="/ai-marketing" replace />} />}
      </Route>
    </Routes>
  );
}
