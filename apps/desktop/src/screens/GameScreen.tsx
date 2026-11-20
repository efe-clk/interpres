import { Crop, Download, Gamepad2, Play, Square } from 'lucide-react'
import { useCallback, useEffect, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { LanguageSelect } from '@/components/common/selects'
import { ErrorNotice, Kbd } from '@/components/common/states'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Progress } from '@/components/ui/progress'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import * as api from '@/lib/api'
import { useApp, useShortcuts } from '@/lib/app-context'
import { formatCombo } from '@/lib/shortcuts'

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4">
      <span className="text-sm text-muted-foreground">{label}</span>
      <div className="flex items-center gap-2">{children}</div>
    </div>
  )
}

export function GameScreen({ active }: { active: boolean }) {
  const { t } = useTranslation()
  const { settings, updateSettings } = useApp()
  const shortcuts = useShortcuts()
  const [ocr, setOcr] = useState<api.OcrStatus | null>(null)
  const [progress, setProgress] = useState<number | null>(null)
  const [running, setRunning] = useState(false)
  const [frame, setFrame] = useState<api.GameFrame | null>(null)
  const [error, setError] = useState<api.ApiError | null>(null)
  const game = settings?.game

  const refreshOcr = useCallback(async () => {
    if (game) setOcr(await api.ocrStatus(game.ocr_model).catch(() => null))
  }, [game])

  useEffect(() => {
    if (!active) return
    void refreshOcr()
    void api.gameStatus().then(setRunning)
  }, [active, refreshOcr])

  useEffect(() => {
    const offs: Promise<() => void>[] = [
      api.onGameFrame(setFrame),
      api.onGameState((s) => {
        setRunning(s.running)
        if (s.error) setError(s.error)
      }),
    ]
    return () => offs.forEach((o) => void o.then((f) => f()))
  }, [])

  if (!settings || !game) return null
  const setGame = (patch: Partial<api.GameSettings>) => void updateSettings({ game: { ...game, ...patch } })

  const run = (fn: () => Promise<void>) => async () => {
    setError(null)
    try {
      await fn()
    } catch (e) {
      setError(e as api.ApiError)
    }
  }

  const download = run(async () => {
    setProgress(0)
    try {
      await api.ocrDownload(game.ocr_model, (done, total) => setProgress(Math.round((done / total) * 100)))
    } finally {
      setProgress(null)
      await refreshOcr()
    }
  })

  const pick = run(async () => {
    const region = await api.gamePickRegion()
    if (region) setGame({ region })
  })

  const toggle = run(async () => {
    if (running) await api.gameStop()
    else await api.gameStart()
    setRunning(await api.gameStatus())
  })

  const ready = !!game.region && !!ocr?.installed
  const blocker = !game.region ? t('game.needRegion') : !ocr?.installed ? t('game.needModel') : null

  return (
    <div className="h-full overflow-auto">
      <div className="mx-auto max-w-2xl space-y-6 px-6 py-6">
        <div className="flex items-start gap-3">
          <Gamepad2 className="mt-0.5 size-5 shrink-0 text-muted-foreground" strokeWidth={1.5} />
          <p className="text-sm text-muted-foreground">{t('game.intro')}</p>
        </div>

        <section className="space-y-3">
          <Row label={t('game.script')}>
            <Select value={game.ocr_model} onValueChange={(v) => setGame({ ocr_model: v as api.OcrModel })}>
              <SelectTrigger className="w-72" aria-label={t('game.script')}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent position="popper">
                <SelectItem value="general">{t('game.scriptGeneral')}</SelectItem>
                <SelectItem value="korean">{t('game.scriptKorean')}</SelectItem>
                <SelectItem value="latin">{t('game.scriptLatin')}</SelectItem>
              </SelectContent>
            </Select>
          </Row>
          <Row label={t('game.model')}>
            {progress !== null ? (
              <div className="w-48 space-y-1">
                <span className="text-xs text-muted-foreground">{t('game.downloading', { percent: progress })}</span>
                <Progress value={progress} className="h-1" />
              </div>
            ) : ocr?.installed ? (
              <Badge variant="secondary">{t('game.modelReady')}</Badge>
            ) : (
              <Button size="sm" variant="outline" onClick={() => void download()}>
                <Download />
                {t('game.download', { size: ocr ? Math.round(ocr.size / 1_000_000) : '…' })}
              </Button>
            )}
          </Row>
          <Row label={t('game.source')}>
            <LanguageSelect kind="source" value={game.source_lang} onChange={(source_lang) => setGame({ source_lang })} className="w-48" label={t('game.source')} />
          </Row>
          <Row label={t('game.target')}>
            <LanguageSelect
              kind="target"
              value={game.target_lang ?? settings.target_lang ?? 'EN-US'}
              onChange={(target_lang) => setGame({ target_lang })}
              className="w-48"
              label={t('game.target')}
            />
          </Row>
          <Row label={t('game.region')}>
            <span className="font-mono text-xs text-muted-foreground">
              {game.region ? `${game.region[2]}×${game.region[3]} @ ${game.region[0]},${game.region[1]}` : t('game.noRegion')}
            </span>
            <Button size="sm" variant="outline" onClick={() => void pick()} title={t('game.pickHint')}>
              <Crop />
              {t('game.pick')}
            </Button>
          </Row>
        </section>

        <section className="flex items-center gap-3 border-t border-border pt-5">
          <Button onClick={() => void toggle()} disabled={!running && !ready}>
            {running ? <Square /> : <Play />}
            {running ? t('game.stop') : t('game.start')}
          </Button>
          <Kbd>{formatCombo(shortcuts.overlay)}</Kbd>
          <span className="ml-auto flex items-center gap-2 text-xs text-muted-foreground">
            <span className={running ? 'size-1.5 rounded-full bg-emerald-500' : 'size-1.5 rounded-full bg-muted-foreground/50'} />
            {running ? t('game.running') : t('game.stopped')}
          </span>
        </section>
        {!running && blocker && <p className="text-xs text-muted-foreground">{blocker}</p>}
        {error && <ErrorNotice error={error} />}

        {running && (
          <section className="space-y-2 rounded-lg border border-border p-4">
            <p className="text-xs text-muted-foreground">{t('game.preview')}</p>
            {frame?.source ? (
              <>
                <p className="text-sm whitespace-pre-wrap text-muted-foreground">{frame.source}</p>
                <p className="text-[15px] whitespace-pre-wrap">{frame.translation}</p>
              </>
            ) : (
              <p className="text-sm text-muted-foreground">{t('game.waiting')}</p>
            )}
          </section>
        )}

        <p className="text-xs text-muted-foreground">{t('game.waylandHint')}</p>
      </div>
    </div>
  )
}
