// DeepL language codes. Names come from Intl.DisplayNames in the UI language.

export const SOURCE_LANGS = [
  'AR', 'BG', 'CS', 'DA', 'DE', 'EL', 'EN', 'ES', 'ET', 'FI', 'FR', 'HU', 'ID', 'IT', 'JA',
  'KO', 'LT', 'LV', 'NB', 'NL', 'PL', 'PT', 'RO', 'RU', 'SK', 'SL', 'SV', 'TR', 'UK', 'ZH',
]

export const TARGET_LANGS = [
  'AR', 'BG', 'CS', 'DA', 'DE', 'EL', 'EN-GB', 'EN-US', 'ES', 'ET', 'FI', 'FR', 'HU', 'ID',
  'IT', 'JA', 'KO', 'LT', 'LV', 'NB', 'NL', 'PL', 'PT-BR', 'PT-PT', 'RO', 'RU', 'SK', 'SL',
  'SV', 'TR', 'UK', 'ZH-HANS', 'ZH-HANT',
]

const FORMALITY = new Set(['DE', 'ES', 'FR', 'IT', 'JA', 'NL', 'PL', 'PT-BR', 'PT-PT', 'RU'])

export function supportsFormality(code: string): boolean {
  return FORMALITY.has(code)
}

export function languageName(code: string, uiLang: string): string {
  try {
    const name = new Intl.DisplayNames([uiLang], { type: 'language' }).of(code.toLowerCase())
    if (name) return name.charAt(0).toLocaleUpperCase(uiLang) + name.slice(1)
  } catch {
    // fall through
  }
  return code
}

/** Maps a source code to the closest target code and back, for swapping. */
export function toTarget(source: string): string {
  if (source === 'EN') return 'EN-US'
  if (source === 'PT') return 'PT-BR'
  if (source === 'ZH') return 'ZH-HANS'
  return source
}

export function toSource(target: string): string {
  return target.split('-')[0]
}
