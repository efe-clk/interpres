import { ClipboardX } from 'lucide-react'
import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { EmptyState, ErrorNotice } from '@/components/common/states'
import { useHotkey } from '@/hooks/use-hotkey'
import * as api from '@/lib/api'
import { applyAppearance } from '@/lib/app-context'
import { languageName } from '@/lib/languages'

/** Small frameless window: translates the clipboard, Enter copies, Esc closes. */
export function QuickTranslate() {
  const { t, i18n } = useTranslation()
  const [source, setSource] = useState('')
  const [target, setTarget] = useState('EN-US')
  const [result, setResult] = useState<api.Translation | null>(null)
  const [error, setError] = useState<api.ApiError | null>(null)
  const [loading, setLoading] = useState(false)
  const [copied, setCopied] = useState(false)

  const run = useCallback(async () => {
    const settings = await api.getSettings()
    applyAppearance(settings)
    const tgt = settings.target_lang ?? 'EN-US'
    setTarget(tgt)
    const text = (await api.readClipboard()).trim()
    setSource(text)
    setResult(null)
    setError(null)
    setCopied(false)
    if (!text) return
    setLoading(true)
    try {
      setResult(await api.translate({ text, source: settings.source_lang, target: tgt, formality: 'default' }))
    } catch (e) {
      setError(e as api.ApiError)
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void run()
    let off = () => {}
    void api.onQuickOpen(() => void run()).then((f) => (off = f))
    return () => off()
  }, [run])

  useHotkey('escape', () => void api.hideCurrentWindow())
  useHotkey('enter', async () => {
    if (!result) return
    await api.writeClipboard(result.text)
    setCopied(true)
    setTimeout(() => void api.hideCurrentWindow(), 300)
  })

  return (
    <div className="flex h-screen flex-col overflow-hidden rounded-lg border border-border bg-background" data-tauri-drag-region>
      <div className="flex items-center justify-between px-4 pt-3 text-xs text-muted-foreground" data-tauri-drag-region>
        <span data-tauri-drag-region>Interpres · {languageName(target, i18n.language)}</span>
        <span data-tauri-drag-region>{copied ? t('common.copied') : t('quick.hint')}</span>
      </div>
      <div className="min-h-0 flex-1 overflow-auto px-4 py-3">
        {!source && !loading ? (
          <EmptyState icon={ClipboardX} title={t('quick.empty')} hint={t('quick.emptyHint')} />
        ) : (
          <>
            <p className="line-clamp-2 text-sm text-muted-foreground">{source}</p>
            <div className="mt-3">
              {error ? (
                <ErrorNotice error={error} />
              ) : loading ? (
                <div className="space-y-2" aria-busy>
                  <div className="h-3.5 w-4/5 animate-pulse rounded bg-muted" />
                  <div className="h-3.5 w-3/5 animate-pulse rounded bg-muted" />
                </div>
              ) : (
                result && <p className="text-[15px] leading-relaxed whitespace-pre-wrap">{result.text}</p>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  )
}
