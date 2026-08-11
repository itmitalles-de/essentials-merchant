const en = {
  "app.title": "ErpLite",

  "common.colorScheme": "Color scheme",
  "common.colorScheme.system": "System",
  "common.colorScheme.light": "Light",
  "common.colorScheme.dark": "Dark",
  "common.language": "Language",
  "common.language.system": "System",
  "common.language.en": "English",
  "common.language.de": "German",

  "nav.dashboard": "Dashboard",

  "dashboard.title": "Dashboard",
  "dashboard.apiStatus": "API status",
  "dashboard.apiStatusOk": "reachable",
  "dashboard.apiStatusError": "unreachable",
};

const de: Record<keyof typeof en, string> = {
  "app.title": "ErpLite",

  "common.colorScheme": "Farbschema",
  "common.colorScheme.system": "System",
  "common.colorScheme.light": "Hell",
  "common.colorScheme.dark": "Dunkel",
  "common.language": "Sprache",
  "common.language.system": "System",
  "common.language.en": "Englisch",
  "common.language.de": "Deutsch",

  "nav.dashboard": "Dashboard",

  "dashboard.title": "Dashboard",
  "dashboard.apiStatus": "API-Status",
  "dashboard.apiStatusOk": "erreichbar",
  "dashboard.apiStatusError": "nicht erreichbar",
};

export type TranslationKey = keyof typeof en;
export const translations = { en, de };
export type Lang = keyof typeof translations;
