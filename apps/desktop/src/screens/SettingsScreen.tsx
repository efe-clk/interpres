import { Check, Terminal } from 'lucide-react'
import { useEffect, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { ENGINES, EngineSelect, LanguageSelect, isEngineAvailable } from '@/components/common/selects'
import { ErrorNotice, Kbd } from '@/components/common/states'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import * as api from '@/lib/api'
import { useApp } from '@/lib/app-context'

const NONE = '__none'
const CLAUDE_MODELS = ['claude-sonnet-5-5', 'claude-opus-5-5', 'claude-haiku-5-5']

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
  const [claudeCode, setClaudeCode] = useState<api.ClaudeCodeStatus | null>(null)
  const [cacheCleared, setCacheCleared] = useState(false)

  useEffect(() => {
    void api.claudeCodeStatus().then(setClaudeCode)
  }, [])
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

        <Section title={t('settings.claude.title')}>
          <p className="text-xs text-muted-foreground">{t('common.comingIn', { milestone: 'M5' })}</p>
          <Row label={t('settings.claude.apiKey')}>
            <Input type="password" disabled className="w-64" placeholder="sk-ant-..." />
          </Row>
          <Row label={t('settings.claude.model')}>
            <Select disabled defaultValue={CLAUDE_MODELS[0]}>
              <SelectTrigger className="w-64">
                <SelectValue />
              </SelectTrigger>
              <SelectContent position="popper">
                {CLAUDE_MODELS.map((m) => (
                  <SelectItem key={m} value={m}>
                    {m}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Row>
          <Row label={t('settings.claude.code')}>
            <span className="flex items-center gap-2 text-sm">
              <Terminal className="size-4 text-muted-foreground" />
              {claudeCode === 'found' ? t('settings.claude.codeFound') : t('settings.claude.codeNotFound')}
            </span>
          </Row>
        </Section>

        <Section title={t('settings.engines.title')}>
          <Row label={t('settings.engines.default')}>
            <EngineSelect value={settings.engine} onChange={(engine) => void updateSettings({ engine })} className="w-48" label={t('settings.engines.default')} />
          </Row>
          <Row label={t('settings.engines.fallback')}>
            <Select
              value={settings.fallback_engine ?? NONE}
              onValueChange={(v) => void updateSettings({ fallback_engine: v === NONE ? null : (v as api.EngineId) })}
            >
              <SelectTrigger className="w-48" aria-label={t('settings.engines.fallback')}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent position="popper">
                <SelectItem value={NONE}>{t('settings.engines.none')}</SelectItem>
                {ENGINES.map((e) => (
                  <SelectItem key={e} value={e} disabled={!isEngineAvailable(e)}>
                    {t(`engine.${e}`)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Row>
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
          {[
            ['shortcuts.palette', 'Ctrl K'],
            ['shortcuts.translate', 'Ctrl Enter'],
            ['shortcuts.swap', 'Ctrl Shift S'],
            ['shortcuts.copy', 'Ctrl Shift C'],
            ['shortcuts.addFiles', 'Ctrl O'],
            ['shortcuts.screens', 'Ctrl 1–5'],
            ['shortcuts.quick', 'Ctrl Alt T'],
          ].map(([label, combo]) => (
            <Row key={label} label={t(label)}>
              <Kbd>{combo}</Kbd>
            </Row>
          ))}
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
