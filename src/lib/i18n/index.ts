import { writable, derived, get } from 'svelte/store';

import en from './locales/en.json';
import ru from './locales/ru.json';

export type Locale = 'en' | 'ru';

export interface LocaleConfig {
  code: Locale;
  name: string;
  nativeName: string;
}

export const locales: LocaleConfig[] = [
  { code: 'en', name: 'English', nativeName: 'English' },
  { code: 'ru', name: 'Russian', nativeName: 'Русский' },
];

const translations: Record<Locale, typeof en> = {
  en,
  ru,
};

const DEFAULT_LOCALE: Locale = 'en';
const STORAGE_KEY = 'comine-locale';

function getInitialLocale(): Locale {
  if (typeof window === 'undefined') return DEFAULT_LOCALE;

  const stored = localStorage.getItem(STORAGE_KEY);
  if (stored && stored in translations) {
    return stored as Locale;
  }

  const browserLang = navigator.language.split('-')[0];
  if (browserLang && browserLang in translations) {
    return browserLang as Locale;
  }

  return DEFAULT_LOCALE;
}

export const locale = writable<Locale>(DEFAULT_LOCALE);

if (typeof window !== 'undefined') {
  locale.set(getInitialLocale());

  locale.subscribe((value) => {
    localStorage.setItem(STORAGE_KEY, value);
    document.documentElement.lang = value;
  });

  window.i18n = {
    t: (key: string) => translate(key as TranslationKeys),
    locale: get(locale),
  };

  locale.subscribe((value) => {
    if (window.i18n) window.i18n.locale = value;
  });
}

function getNestedValue(obj: Record<string, unknown>, path: string): string | undefined {
  const keys = path.split('.');
  let current: unknown = obj;

  for (const key of keys) {
    if (current === null || current === undefined || typeof current !== 'object') {
      return undefined;
    }
    current = (current as Record<string, unknown>)[key];
  }

  return typeof current === 'string' ? current : undefined;
}

function interpolate(str: string, vars?: Record<string, string | number>): string {
  if (!vars) return str;

  return str.replace(/\{(\w+)\}/g, (_, key) => {
    return vars[key]?.toString() ?? `{${key}}`;
  });
}

import type { TranslationKeys } from './keys';

export type TranslationFunction = (
  key: TranslationKeys,
  vars?: Record<string, string | number>
) => string;

function resolveTranslation(
  loc: Locale,
  key: TranslationKeys,
  vars?: Record<string, string | number>
): string {
  const translation = getNestedValue(translations[loc], key);
  if (translation !== undefined) return interpolate(translation, vars);
  const fallback = getNestedValue(translations[DEFAULT_LOCALE], key);
  if (fallback !== undefined) return interpolate(fallback, vars);
  console.warn(`[i18n] Missing translation: ${key}`);
  return key;
}

export const t = derived<typeof locale, TranslationFunction>(locale, ($locale) => {
  return (key: TranslationKeys, vars?: Record<string, string | number>) =>
    resolveTranslation($locale, key, vars);
});

export function setLocale(newLocale: Locale): void {
  if (newLocale in translations) {
    locale.set(newLocale);
  } else {
    console.warn(`[i18n] Unknown locale: ${newLocale}`);
  }
}

export function getLocale(): Locale {
  return get(locale);
}

export function translate(key: TranslationKeys, vars?: Record<string, string | number>): string {
  return resolveTranslation(get(locale), key, vars);
}

export type Translations = typeof en;
