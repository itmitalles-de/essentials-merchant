export function isMantlePilotExperience(): boolean {
  return window.location.hostname === "ai-marketing.mantle-climbing.de"
    || window.location.pathname === "/ai-marketing"
    || window.location.pathname.startsWith("/ai-marketing/");
}
