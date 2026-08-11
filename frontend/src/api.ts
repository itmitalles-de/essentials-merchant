const API_BASE = import.meta.env.VITE_API_BASE ?? "/api";

export async function getHealth(): Promise<{ status: string }> {
  const res = await fetch(`${API_BASE}/health`);
  if (!res.ok) throw new Error(`health check failed: ${res.status}`);
  return res.json();
}
