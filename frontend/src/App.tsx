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

function RequireAuth({ children }: { children: ReactNode }) {
  const { isAuthenticated, loading } = useAuth();
  if (loading) return null;
  if (!isAuthenticated) return <Navigate to="/login" replace />;
  return <>{children}</>;
}

function HomeRoute() {
  return window.location.hostname === "ai-marketing.mantle-climbing.de"
    ? <Navigate to="/ai-marketing" replace />
    : <Dashboard />;
}

export default function App() {
  return (
    <Routes>
      <Route path="/login" element={<Login />} />
      <Route
        path="/"
        element={
          <RequireAuth>
            <Layout />
          </RequireAuth>
        }
      >
        <Route index element={<HomeRoute />} />
        <Route path="customers" element={<Customers />} />
        <Route path="invoices" element={<Invoices />} />
        <Route path="invoices/:id" element={<InvoiceDetail />} />
        <Route path="articles" element={<Articles />} />
        <Route path="articles/:id" element={<ArticleDetail />} />
        <Route path="sales-orders" element={<SalesOrders />} />
        <Route path="sales-orders/:id" element={<SalesOrderDetail />} />
        <Route path="settings" element={<Settings />} />
        <Route path="admin-center" element={<AdminCenter />} />
        <Route path="integration-diagnostics" element={<IntegrationDiagnostics />} />
        <Route path="marketplace" element={<MarketplaceIntelligence />} />
        <Route path="ai-marketing" element={<MarketplaceIntelligence aiFirst />} />
      </Route>
    </Routes>
  );
}
