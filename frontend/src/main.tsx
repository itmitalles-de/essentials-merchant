import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import App from "./App";
import { ThemeProvider } from "./contexts/ThemeContext";
import { LanguageProvider } from "./contexts/LanguageContext";
import { AuthProvider } from "./contexts/AuthContext";
import { isMantlePilotExperience } from "./pilot";
import "@itmitalles-de/simple-business-design-system/tokens.css";
import "./theme.css";

if (isMantlePilotExperience()) {
  document.title = "Mantle · AI Marketing";
  document
    .querySelector<HTMLLinkElement>('link[rel="icon"]')
    ?.setAttribute("href", "/ai-marketing-icon.svg");
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ThemeProvider>
      <LanguageProvider>
        <BrowserRouter>
          <AuthProvider>
            <App />
          </AuthProvider>
        </BrowserRouter>
      </LanguageProvider>
    </ThemeProvider>
  </StrictMode>
);
