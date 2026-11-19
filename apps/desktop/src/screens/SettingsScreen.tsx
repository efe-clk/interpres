import { Check } from 'lucide-react'
import { useEffect, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { LanguageSelect } from '@/components/common/selects'
import { ErrorNotice, Kbd } from '@/components/common/states'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import * as api from '@/lib/api'
import { useApp } from '@/lib/app-context'
import { DEFAULT_SHORTCUTS, comboFromEvent, formatCombo, hasModifier } from '@/lib/shortcuts'
import { cn } from '@/lib/utils'


function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="space-y-3 border-b border-border py-6 first:pt-0 last:border-0">
      <h2 className="text-sm font-medium">{title}</h2>
      {children}
    </section>
  )
}

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4">
      <span className="text-sm text-muted-foreground">{label}</span>
      <div className="flex items-center gap-2">{children}</div>
    </div>
  )
}

export function SettingsScreen() {
  const { t, i18n } = useTranslation()
  const { settings, updateSettings, keyStatus, refreshKey } = useApp()
  const [key, setKey] = useState('')
  const [verifying, setVerifying] = useState(false)
  const [keyError, setKeyError] = useState<api.ApiError | null>(null)
  const [usage, setUsage] = useState<api.Usage | null>(null)
  const [cacheCleared, setCacheCleared] = useState(false)

  useEffect(() => {
    if (keyStatus.deepl) void api.deeplUsage().then(setUsage, () => setUsage(null))
    else setUsage(null)
  }, [keyStatus.deepl])

  if (!settings) return null

  const saveKey = async () => {
    setVerifying(true)
    setKeyError(null)
    try {
      await api.setDeeplKey(key)
      setKey('')
      await refreshKey()
    } catch (e) {
      setKeyError(e as api.ApiError)
    } finally {
      setVerifying(false)
    }
  }

  const num = new Intl.NumberFormat(i18n.language)

  return (
    <div className="h-full overflow-auto">
      <div className="mx-auto max-w-2xl px-6 py-6">
        <Section title={t('settings.deepl.title')}>
          {keyStatus.deepl ? (
            <div className="flex items-center justify-between gap-4">
              <div className="space-y-1">
                <p className="flex items-center gap-2 text-sm">
                  <Check className="size-4 text-primary" />
                  {t('settings.deepl.saved', { plan: t(`status.${keyStatus.deepl}`) })}
                </p>
                {usage && (
                  <p className="text-xs text-muted-foreground">
                    {t('settings.deepl.usage', { used: num.format(usage.character_count), limit: num.format(usage.character_limit) })}
                  </p>
                )}
              </div>
              <Button
                variant="ghost"
                size="sm"
                onClick={async () => {
                  await api.deleteDeeplKey()
                  await refreshKey()
                }}
              >
                {t('settings.deepl.remove')}
              </Button>
            </div>
          ) : (
            <form
              className="flex gap-2"
              onSubmit={(e) => {
                e.preventDefault()
                void saveKey()
              }}
            >
              <Input
                type="password"
                value={key}
                onChange={(e) => setKey(e.target.value)}
                placeholder={t('settings.deepl.keyPlaceholder')}
                aria-label={t('settings.deepl.key')}
                autoComplete="off"
              />
              {key.trim() && <Badge variant="secondary">{key.trim().endsWith(':fx') ? 'Free' : 'Pro'}</Badge>}
              <Button type="submit" disabled={!key.trim() || verifying}>
                {verifying ? t('settings.deepl.verifying') : t('settings.deepl.verify')}
              </Button>
            </form>
          )}
          {keyError && <ErrorNotice error={keyError} />}
        </Section>

        <Section title={t('settings.languages.title')}>
          <Row label={t('settings.languages.source')}>
            <LanguageSelect kind="source" value={settings.source_lang} onChange={(source_lang) => void updateSettings({ source_lang })} className="w-48" label={t('settings.languages.source')} />
          </Row>
          <Row label={t('settings.languages.target')}>
            <LanguageSelect kind="target" value={settings.target_lang ?? 'EN-US'} onChange={(target_lang) => void updateSettings({ target_lang })} className="w-48" label={t('settings.languages.target')} />
          </Row>
        </Section>

        <Section title={t('settings.appearance.title')}>
          <Row label={t('settings.appearance.theme')}>
            <Select value={settings.ui.theme} onValueChange={(theme) => void updateSettings({ ui: { ...settings.ui, theme: theme as 'dark' | 'light' } })}>
              <SelectTrigger className="w-48" aria-label={t('settings.appearance.theme')}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent position="popper">
                <SelectItem value="dark">{t('settings.appearance.dark')}</SelectItem>
                <SelectItem value="light">{t('settings.appearance.light')}</SelectItem>
              </SelectContent>
            </Select>
          </Row>
          <Row label={t('settings.appearance.uiLanguage')}>
            <Select value={settings.ui.language} onValueChange={(language) => void updateSettings({ ui: { ...settings.ui, language: language as 'tr' | 'en' } })}>
              <SelectTrigger className="w-48" aria-label={t('settings.appearance.uiLanguage')}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent position="popper">
                <SelectItem value="tr">Türkçe</SelectItem>
                <SelectItem value="en">English</SelectItem>
              </SelectContent>
            </Select>
          </Row>
        </Section>

        <Section title={t('settings.shortcuts.title')}>
          <ShortcutsEditor
            shortcuts={settings.ui.shortcuts}
            onChange={(shortcuts) => void updateSettings({ ui: { ...settings.ui, shortcuts } })}
          />
          <Row label={t('shortcuts.screens')}>
            <Kbd>Ctrl 1–5</Kbd>
          </Row>
          <p className="text-xs text-muted-foreground">{t('shortcuts.quickWayland')}</p>
        </Section>

        <Section title={t('settings.cache.title')}>
          <Row label={cacheCleared ? t('settings.cache.cleared') : ''}>
            <Button
              variant="outline"
              size="sm"
              onClick={async () => {
                await api.clearCache()
                setCacheCleared(true)
              }}
            >
              {t('settings.cache.clear')}
            </Button>
          </Row>
        </Section>
      </div>
    </div>
  )
}

const SHORTCUT_LABELS: [keyof api.Shortcuts, string][] = [
  ['palette', 'shortcuts.palette'],
  ['translate', 'shortcuts.translate'],
  ['swap', 'shortcuts.swap'],
  ['copy', 'shortcuts.copy'],
  ['add_files', 'shortcuts.addFiles'],
  ['quick', 'shortcuts.quick'],
  ['toggle', 'shortcuts.toggle'],
]

function ShortcutsEditor({ shortcuts, onChange }: { shortcuts: api.Shortcuts; onChange: (s: api.Shortcuts) => void }) {
  const { t } = useTranslation()
  const [recording, setRecording] = useState<keyof api.Shortcuts | null>(null)
  const [error, setError] = useState('')

  useEffect(() => {
    if (!recording) return
    // Capture phase, so the app's own shortcuts do not fire while recording.
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault()
      e.stopImmediatePropagation()
      if (e.key === 'Escape') {
        setRecording(null)
        setError('')
        return
      }
      const combo = comboFromEvent(e)
      if (!combo) return
      if (!hasModifier(combo)) {
        setError(t('settings.shortcuts.needModifier'))
        return
      }
      const clash = SHORTCUT_LABELS.find(([k]) => k !== recording && shortcuts[k] === combo)
      if (clash) {
        setError(t('settings.shortcuts.conflict', { action: t(clash[1]) }))
        return
      }
      onChange({ ...shortcuts, [recording]: combo })
      setRecording(null)
      setError('')
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [recording, shortcuts, onChange, t])

  const isDefault = SHORTCUT_LABELS.every(([k]) => shortcuts[k] === DEFAULT_SHORTCUTS[k])

  return (
    <>
      {SHORTCUT_LABELS.map(([key, label]) => (
        <Row key={key} label={t(label)}>
          <button
            onClick={() => {
              setError('')
              setRecording(recording === key ? null : key)
            }}
            title={t('settings.shortcuts.hint')}
            className={cn(
              'rounded border border-border bg-muted px-1.5 py-0.5 font-mono text-[11px] text-muted-foreground hover:text-foreground',
              recording === key && 'border-primary text-foreground',
            )}
          >
            {recording === key ? t('settings.shortcuts.record') : formatCombo(shortcuts[key])}
          </button>
        </Row>
      ))}
      {error && <p className="text-xs text-destructive">{error}</p>}
      {!isDefault && (
        <div className="flex justify-end">
          <Button variant="ghost" size="sm" onClick={() => onChange(DEFAULT_SHORTCUTS)}>
            {t('settings.shortcuts.reset')}
          </Button>
        </div>
      )}
    </>
  )
}
