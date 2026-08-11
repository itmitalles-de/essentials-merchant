import { createContext, useContext, useEffect, useState } from "react";
import type { ReactNode } from "react";
import { translations } from "../i18n/translations";
import type { Lang, TranslationKey } from "../i18n/translations";

type LanguageChoice = "system" | Lang;

interface LanguageContextValue {
  choice: LanguageChoice;
  lang: Lang;
  setChoice: (choice: LanguageChoice) => void;
  t: (key: TranslationKey) => string;
}

const LanguageContext = createContext<LanguageContextValue | null>(null);

const STORAGE_KEY = "erplite-lang";

function systemLang(): Lang {
  return navigator.language.toLowerCase().startsWith("de") ? "de" : "en";
}

export function LanguageProvider({ children }: { children: ReactNode }) {
  const [choice, setChoiceState] = useState<LanguageChoice>(() => {
    const stored = localStorage.getItem(STORAGE_KEY);
    return stored === "en" || stored === "de" || stored === "system" ? stored : "system";
  });

  useEffect(() => {
    localStorage.setItem(STORAGE_KEY, choice);
  }, [choice]);

  const lang = choice === "system" ? systemLang() : choice;
  const setChoice = (next: LanguageChoice) => setChoiceState(next);
  const t = (key: TranslationKey) => translations[lang][key];

  return (
    <LanguageContext.Provider value={{ choice, lang, setChoice, t }}>
      {children}
    </LanguageContext.Provider>
  );
}

export function useLanguage() {
  const ctx = useContext(LanguageContext);
  if (!ctx) throw new Error("useLanguage must be used within LanguageProvider");
  return ctx;
}
