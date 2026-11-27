// SPDX-License-Identifier: GPL-3.0-or-later
import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';

import de from '../locales/de.json';
import en from '../locales/en.json';

export const LANGUAGES = ['en', 'de'] as const;

function detect(): string {
  try {
    const saved = localStorage.getItem('ssh-client:lang');
    if (saved && (LANGUAGES as readonly string[]).includes(saved)) return saved;
  } catch {
    // storage may be unavailable
  }
  const nav = navigator.language.slice(0, 2).toLowerCase();
  return (LANGUAGES as readonly string[]).includes(nav) ? nav : 'en';
}

export async function initI18n(): Promise<void> {
  await i18n.use(initReactI18next).init({
    resources: { en: { translation: en }, de: { translation: de } },
    lng: detect(),
    fallbackLng: 'en',
    interpolation: { escapeValue: false },
    returnNull: false,
  });
  i18n.on('languageChanged', (lng) => {
    document.documentElement.lang = lng;
    try {
      localStorage.setItem('ssh-client:lang', lng);
    } catch {
      // ignore
    }
  });
  document.documentElement.lang = i18n.language;
}

export default i18n;
