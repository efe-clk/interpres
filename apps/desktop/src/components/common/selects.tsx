import { useTranslation } from 'react-i18next'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import type { EngineId } from '@/lib/api'
import { SOURCE_LANGS, TARGET_LANGS, languageName } from '@/lib/languages'

const AUTO = '__auto'

export function LanguageSelect({
  kind,
  value,
  onChange,
  className,
  label,
}: {
  kind: 'source' | 'target'
  value: string | null
  onChange: (v: string | null) => void
  className?: string
  label: string
}) {
  const { t, i18n } = useTranslation()
  const codes = kind === 'source' ? SOURCE_LANGS : TARGET_LANGS
  const items = codes
    .map((c) => ({ code: c, name: languageName(c, i18n.language) }))
    .sort((a, b) => a.name.localeCompare(b.name, i18n.language))
  return (
    <Select value={value ?? AUTO} onValueChange={(v) => onChange(v === AUTO ? null : v)}>
      <SelectTrigger className={className} aria-label={label}>
        <SelectValue />
      </SelectTrigger>
      <SelectContent position="popper" className="max-h-80">
        {kind === 'source' && <SelectItem value={AUTO}>{t('text.auto')}</SelectItem>}
        {items.map((l) => (
          <SelectItem key={l.code} value={l.code}>
            {l.name}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}

export const ENGINES: EngineId[] = ['deepl', 'claude-api', 'claude-code']

/** Claude engines arrive in M5. */
export const isEngineAvailable = (e: EngineId) => e === 'deepl'

export function EngineSelect({
  value,
  onChange,
  className,
  label,
}: {
  value: EngineId
  onChange: (v: EngineId) => void
  className?: string
  label: string
}) {
  const { t } = useTranslation()
  return (
    <Select value={value} onValueChange={(v) => onChange(v as EngineId)}>
      <SelectTrigger className={className} aria-label={label}>
        <SelectValue />
      </SelectTrigger>
      <SelectContent position="popper">
        {ENGINES.map((e) => (
          <SelectItem key={e} value={e} disabled={!isEngineAvailable(e)}>
            {t(`engine.${e}`)}
            {!isEngineAvailable(e) && (
              <span className="ml-2 text-xs text-muted-foreground">{t('common.comingIn', { milestone: 'M5' })}</span>
            )}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}
