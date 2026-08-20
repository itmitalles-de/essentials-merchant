import { useEffect, useState } from "react";
import { api } from "../api";
import type { AmazonPilotStatus } from "../types";

export function usePilotStatus() {
  const [pilot, setPilot] = useState<AmazonPilotStatus | null>(null);

  useEffect(() => {
    api.get<AmazonPilotStatus>("/pilot/status").then(setPilot).catch(() => setPilot(null));
  }, []);

  return pilot;
}
