import { ArrowLeftRight, BookA, Check, Copy, X } from 'lucide-react'
import { useCallback, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { LanguageSelect } from '@/components/common/selects'
import { ErrorNotice, Kbd } from '@/components/common/states'
import { TEXT_EVENTS } from '@/components/shell/CommandPalette'
import { Button } from '@/components/ui/button'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { useHotkey } from '@/hooks/use-hotkey'
import * as api from '@/lib/api'
import { useApp, useShortcuts } from '@/lib/app-context'
import { formatCombo } from '@/lib/shortcuts'
import { languageName, supportsFormality, toSource, toTarget } from '@/lib/languages'

export function TextScreen({ active }: { active: boolean }) {
  const { t, i18n } = useTranslation()
  const { settings, textPreset } = useApp()
  const shortcuts = useShortcuts()

  const [source, setSource] = useState<string | null>(null)
  const [target, setTarget] = useState('EN-US')
  const [formality, setFormality] = useState<api.Formality>('default')
  const [input, setInput] = useState('')
  const [result, setResult] = useState<api.Translation | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<api.ApiError | null>(null)
  const [copied, setCopied] = useState(false)
  const inputRef = useRef<HTMLTextAreaElement>(null)

  // Defaults from settings, once loaded.
  const initialized = useRef(false)
  useEffect(() => {
    if (!settings || initialized.current) return
    initialized.current = true
    setSource(settings.source_lang)
    setTarget(settings.target_lang ?? 'EN-US')
  }, [settings])

  // Reopen from history.
  useEffect(() => {
    if (!textPreset) return
    setSource(textPreset.source_lang)
    setTarget(textPreset.target_lang)
    setInput(textPreset.source_text)
    setResult({ text: textPreset.target_text, detected_source: textPreset.detected_source, engine: textPreset.engine })
    setError(null)
  }, [textPreset])

  useEffect(() => {
    if (active) inputRef.current?.focus()
  }, [active])

  const formalityOk = supportsFormality(target)

  // Number of glossary terms that will apply (only with an explicit source language).
  const [glossaryCount, setGlossaryCount] = useState(0)
  useEffect(() => {
    if (!active || !source) {
      setGlossaryCount(0)
      return
    }
    api.listTerms(toSource(source), toSource(target)).then(
      (terms) => setGlossaryCount(terms.length),
      () => setGlossaryCount(0),
    )
  }, [active, source, target])

  const run = useCallback(async () => {
    if (!input.trim() || loading) return
    setLoading(true)
    setError(null)
    try {
      setResult(
        await api.translate({
          text: input,
          source,
          target,
          formality: formalityOk ? formality : 'default',
        }),
      )
    } catch (e) {
      setResult(null)
      setError(e as api.ApiError)
    } finally {
      setLoading(false)
    }
  }, [input, loading, source, target, formality, formalityOk])

  const swap = useCallback(() => {
    const from = source ?? result?.detected_source ?? null
    if (!from) return
    setSource(toSource(target))
    setTarget(toTarget(from))
    if (result) {
      setInput(result.text)
      setResult(null)
    }
  }, [source, target, result])

  const copy = useCallback(async () => {
    if (!result) return
    await api.writeClipboard(result.text)
    setCopied(true)
    setTimeout(() => setCopied(false), 1200)
  }, [result])

  useHotkey(shortcuts.translate, () => void run(), active)
  useHotkey(shortcuts.swap, swap, active)
  useHotkey(shortcuts.copy, () => void copy(), active)

  useEffect(() => {
    const onTranslate = () => void run()
    window.addEventListener(TEXT_EVENTS.translate, onTranslate)
    window.addEventListener(TEXT_EVENTS.swap, swap)
    return () => {
      window.removeEventListener(TEXT_EVENTS.translate, onTranslate)
      window.removeEventListener(TEXT_EVENTS.swap, swap)
    }
  }, [run, swap])

  return (
    <div className="flex h-full flex-col">
      <header className="flex flex-wrap items-center gap-2 border-b border-border px-5 py-3">
        <LanguageSelect kind="source" value={source} onChange={setSource} className="w-44" label={t('settings.languages.source')} />
        <Tooltip>
          <TooltipTrigger asChild>
            <Button variant="ghost" size="icon" onClick={swap} aria-label={t('text.swap')}>
              <ArrowLeftRight />
            </Button>
          </TooltipTrigger>
          <TooltipContent>
            {t('text.swap')} · {formatCombo(shortcuts.swap)}
          </TooltipContent>
        </Tooltip>
        <LanguageSelect
          kind="target"
          value={target}
          onChange={(v) => v && setTarget(v)}
          className="w-44"
          label={t('settings.languages.target')}
        />
        <div className="ml-auto flex items-center gap-2">
          {glossaryCount > 0 && (
            <span className="flex items-center gap-1 text-xs text-muted-foreground">
              <BookA className="size-3.5" />
              {t('glossary.active', { count: glossaryCount })}
            </span>
          )}
          <Select value={formalityOk ? formality : 'default'} onValueChange={(v) => setFormality(v as api.Formality)} disabled={!formalityOk}>
            <SelectTrigger className="w-32" aria-label={t('text.formality.label')} title={formalityOk ? undefined : t('text.formality.unsupported')}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent position="popper">
              <SelectItem value="default">{t('text.formality.default')}</SelectItem>
              <SelectItem value="more">{t('text.formality.more')}</SelectItem>
              <SelectItem value="less">{t('text.formality.less')}</SelectItem>
            </SelectContent>
          </Select>
        </div>
      </header>

      <div className="grid min-h-0 flex-1 grid-cols-2">
        <section className="flex min-h-0 flex-col border-r border-border">
          <textarea
            ref={inputRef}
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder={t('text.sourcePlaceholder')}
            spellCheck={false}
            className="min-h-0 flex-1 resize-none bg-transparent px-5 py-4 text-[15px] leading-relaxed outline-none placeholder:text-muted-foreground/70"
          />
          <footer className="flex items-center gap-2 px-5 py-3 text-xs text-muted-foreground">
            <span className="tabular-nums">{t('text.chars', { count: input.length })}</span>
            {input && (
              <Button variant="ghost" size="icon-xs" onClick={() => { setInput(''); setResult(null); setError(null) }} aria-label={t('text.clear')}>
                <X />
              </Button>
            )}
            <Button className="ml-auto" size="sm" onClick={() => void run()} disabled={!input.trim() || loading}>
              {loading ? t('text.translating') : t('text.translate')}
              <span className="ml-1 font-mono text-[11px] opacity-70">{formatCombo(shortcuts.translate)}</span>
            </Button>
          </footer>
        </section>

        <section className="flex min-h-0 flex-col bg-muted/20" aria-live="polite">
          <div className="min-h-0 flex-1 overflow-auto px-5 py-4">
            {error ? (
              <ErrorNotice error={error} onRetry={() => void run()} />
            ) : loading ? (
              <div className="space-y-2.5 pt-1" aria-busy>
                <div className="h-3.5 w-4/5 animate-pulse rounded bg-muted" />
                <div className="h-3.5 w-3/5 animate-pulse rounded bg-muted" />
                <div className="h-3.5 w-2/3 animate-pulse rounded bg-muted" />
              </div>
            ) : result ? (
              <p className="text-[15px] leading-relaxed whitespace-pre-wrap select-text">{result.text}</p>
            ) : (
              <p className="text-[15px] text-muted-foreground/70">{t('text.outputEmpty')}</p>
            )}
          </div>
          <footer className="flex h-[52px] items-center gap-2 px-5 text-xs text-muted-foreground">
            {result && !source && result.detected_source && (
              <span>{t('text.detected', { lang: languageName(result.detected_source, i18n.language) })}</span>
            )}
            {result && (
              <Button variant="ghost" size="sm" className="ml-auto" onClick={() => void copy()}>
                {copied ? <Check /> : <Copy />}
                {copied ? t('common.copied') : t('common.copy')}
                <Kbd>{formatCombo(shortcuts.copy)}</Kbd>
              </Button>
            )}
          </footer>
        </section>
      </div>
    </div>
  )
}
