import { AudioLines, Crop, Download, Play, ScanText, Square } from 'lucide-react'
import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { LanguageSelect } from '@/components/common/selects'
import { ErrorNotice, Kbd } from '@/components/common/states'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Progress } from '@/components/ui/progress'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Slider } from '@/components/ui/slider'
import { Switch } from '@/components/ui/switch'
import * as api from '@/lib/api'
import { useApp, useShortcuts } from '@/lib/app-context'
import { captionCss } from '@/lib/caption'
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
  const [speech, setSpeech] = useState<api.SpeechStatus | null>(null)
  const [progress, setProgress] = useState<number | null>(null)
  const [running, setRunning] = useState(false)
  const [frame, setFrame] = useState<api.GameFrame | null>(null)
  const [error, setError] = useState<api.ApiError | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const restartTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const game = settings?.game

  const refreshOcr = useCallback(async () => {
    if (game) setOcr(await api.ocrStatus(game.ocr_model).catch(() => null))
  }, [game])

  const speechModel = game?.speech.model
  const refreshSpeech = useCallback(async () => {
    if (speechModel) setSpeech(await api.speechStatus(speechModel).catch(() => null))
  }, [speechModel])

  useEffect(() => {
    if (!active) return
    void refreshOcr()
    void refreshSpeech()
    void api.gameStatus().then(setRunning)
  }, [active, refreshOcr, refreshSpeech])

  useEffect(() => {
    const offs: Promise<() => void>[] = [
      api.onGameFrame(setFrame),
      api.onGameState((s) => {
        setRunning(s.running)
        if (s.error) setError(s.error)
        if (s.notice) setNotice(s.notice)
        if (!s.running) setNotice(null)
      }),
    ]
    return () => offs.forEach((o) => void o.then((f) => f()))
  }, [])

  if (!settings || !game) return null
  const setGame = (patch: Partial<api.GameSettings>) => void updateSettings({ game: { ...game, ...patch } })
  const setSpeechSettings = (patch: Partial<api.SpeechSettings>) => setGame({ speech: { ...game.speech, ...patch } })
  const caption = game.caption
  const isSpeech = game.input === 'speech'

  // Saves at once; a running overlay restarts shortly after the last change to
  // pick the new look up (restarting reloads the OCR model, so not on every step).
  const setCaption = (patch: Partial<api.CaptionStyle>) => {
    setGame({ caption: { ...caption, ...patch } })
    if (!running) return
    if (restartTimer.current) clearTimeout(restartTimer.current)
    restartTimer.current = setTimeout(() => {
      void api
        .gameStop()
        .then(() => api.gameStart())
        .catch((e) => setError(e as api.ApiError))
    }, 700)
  }

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

  const downloadSpeech = run(async () => {
    if (!speech) return
    setProgress(0)
    try {
      await api.speechDownload(speech.model, (done, total) => setProgress(Math.round((done / total) * 100)))
    } finally {
      setProgress(null)
      await refreshSpeech()
    }
  })

  const pick = run(async () => {
    const region = await api.gamePickRegion()
    if (region) setGame({ region })
  })

  const pickArea = run(async () => {
    const area = await api.gamePickRegion()
    if (area) setCaption({ area, placement: 'custom' })
  })

  const css = captionCss(caption)

  const toggle = run(async () => {
    if (running) await api.gameStop()
    else await api.gameStart()
    setRunning(await api.gameStatus())
  })

  const ready = isSpeech ? !!speech?.installed : !!game.region && !!ocr?.installed
  const blocker = isSpeech
    ? speech?.installed
      ? null
      : t('game.needSpeechModel')
    : !game.region
      ? t('game.needRegion')
      : !ocr?.installed
        ? t('game.needModel')
        : null

  const modelStatus = (installed: boolean | undefined, size: number | undefined, onDownload: () => void) =>
    progress !== null ? (
      <div className="w-48 space-y-1">
        <span className="text-xs text-muted-foreground">{t('game.downloading', { percent: progress })}</span>
        <Progress value={progress} className="h-1" />
      </div>
    ) : installed ? (
      <Badge variant="secondary">{t('game.modelReady')}</Badge>
    ) : (
      <Button size="sm" variant="outline" onClick={onDownload}>
        <Download />
        {t('game.download', { size: size ? Math.round(size / 1_000_000) : '…' })}
      </Button>
    )

  return (
    <div className="h-full overflow-auto">
      <div className="mx-auto max-w-2xl space-y-6 px-6 py-6">
        <div className="flex items-start gap-3">
          {isSpeech ? (
            <AudioLines className="mt-0.5 size-5 shrink-0 text-muted-foreground" strokeWidth={1.5} />
          ) : (
            <ScanText className="mt-0.5 size-5 shrink-0 text-muted-foreground" strokeWidth={1.5} />
          )}
          <p className="text-sm text-muted-foreground">{isSpeech ? t('game.introSpeech') : t('game.intro')}</p>
        </div>

        <section className="space-y-3">
          <Row label={t('game.input')}>
            <Select value={game.input} disabled={running} onValueChange={(v) => setGame({ input: v as api.GameSettings['input'] })}>
              <SelectTrigger className="w-72" aria-label={t('game.input')}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent position="popper">
                <SelectItem value="screen">{t('game.inputScreen')}</SelectItem>
                <SelectItem value="speech">{t('game.inputSpeech')}</SelectItem>
              </SelectContent>
            </Select>
          </Row>
        </section>

        {isSpeech ? (
          <section className="space-y-3">
            <Row label={t('game.speechSource')}>
              <Select value={game.speech.source} onValueChange={(v) => setSpeechSettings({ source: v as api.SpeechSettings['source'] })}>
                <SelectTrigger className="w-72" aria-label={t('game.speechSource')}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent position="popper">
                  <SelectItem value="system">{t('game.speechSystem')}</SelectItem>
                  <SelectItem value="microphone">{t('game.speechMic')}</SelectItem>
                </SelectContent>
              </Select>
            </Row>
            <Row label={t('game.speechLanguage')}>
              <LanguageSelect
                kind="source"
                value={game.speech.language}
                onChange={(language) => setSpeechSettings({ language })}
                className="w-48"
                label={t('game.speechLanguage')}
              />
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
            <Row label={t('game.speechModel')}>
              <Select value={game.speech.model} onValueChange={(v) => setSpeechSettings({ model: v as api.SpeechSettings['model'] })}>
                <SelectTrigger className="w-72" aria-label={t('game.speechModel')}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent position="popper">
                  <SelectItem value="auto">{t('game.speechModelAuto', { model: speech?.gpu ? 'Turbo' : 'Small' })}</SelectItem>
                  <SelectItem value="small">{t('game.speechModelSmall')}</SelectItem>
                  <SelectItem value="turbo">{t('game.speechModelTurbo')}</SelectItem>
                </SelectContent>
              </Select>
            </Row>
            <Row label={t('game.speechModelFile')}>{modelStatus(speech?.installed, speech?.size, () => void downloadSpeech())}</Row>
            {speech && (
              <p className="text-xs text-muted-foreground">{speech.gpu ? t('game.gpu', { name: speech.gpu }) : t('game.noGpu')}</p>
            )}
          </section>
        ) : (
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
          <Row label={t('game.model')}>{modelStatus(ocr?.installed, ocr?.size, () => void download())}</Row>
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
        )}

        <section className="space-y-3 border-t border-border pt-5">
          <h2 className="text-sm font-medium">{t('game.look')}</h2>
          {!isSpeech && (
          <Row label={t('game.placement')}>
            {caption.placement === 'custom' && (
              <>
                <span className="font-mono text-xs text-muted-foreground">
                  {caption.area ? `${caption.area[2]}×${caption.area[3]} @ ${caption.area[0]},${caption.area[1]}` : t('game.noArea')}
                </span>
                <Button size="sm" variant="outline" onClick={() => void pickArea()}>
                  <Crop />
                  {t('game.pickArea')}
                </Button>
              </>
            )}
            <Select
              value={caption.placement}
              onValueChange={(v) => {
                const placement = v as api.CaptionPlacement
                if (placement === 'custom' && !caption.area) void pickArea()
                else setCaption({ placement })
              }}
            >
              <SelectTrigger className="w-48" aria-label={t('game.placement')}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent position="popper">
                <SelectItem value="below">{t('game.placementBelow')}</SelectItem>
                <SelectItem value="above">{t('game.placementAbove')}</SelectItem>
                <SelectItem value="custom">{t('game.placementCustom')}</SelectItem>
              </SelectContent>
            </Select>
          </Row>
          )}
          <Row label={t('game.showOriginal')}>
            <Switch
              checked={caption.show_original}
              onCheckedChange={(show_original) => setCaption({ show_original })}
              aria-label={t('game.showOriginal')}
            />
          </Row>
          <Row label={t('game.fontSize')}>
            <span className="w-12 text-right font-mono text-xs text-muted-foreground">
              {caption.font_size ? `${caption.font_size}px` : ''}
            </span>
            <Slider
              className="w-40"
              min={12}
              max={48}
              step={1}
              disabled={caption.font_size === null}
              value={[caption.font_size ?? 24]}
              onValueChange={([v]) => setCaption({ font_size: v })}
              aria-label={t('game.fontSize')}
            />
            <label className="flex items-center gap-2 text-sm text-muted-foreground">
              <Switch checked={caption.font_size === null} onCheckedChange={(auto) => setCaption({ font_size: auto ? null : 24 })} />
              {t('game.fontAuto')}
            </label>
          </Row>
          <Row label={t('game.background')}>
            <input
              type="color"
              value={caption.background}
              onChange={(e) => setCaption({ background: e.target.value })}
              aria-label={t('game.background')}
              className="h-8 w-10 cursor-pointer rounded-md border border-border bg-transparent p-0.5"
            />
          </Row>
          <Row label={t('game.opacity')}>
            <span className="w-12 text-right font-mono text-xs text-muted-foreground">{caption.opacity}%</span>
            <Slider
              className="w-40"
              min={0}
              max={100}
              step={5}
              value={[caption.opacity]}
              onValueChange={([v]) => setCaption({ opacity: v })}
              aria-label={t('game.opacity')}
            />
          </Row>
          {/* Preview over a busy backdrop, like a game scene. */}
          <div
            className="rounded-lg p-4"
            style={{ background: 'linear-gradient(135deg, #2a3a5a, #5a3a2a 50%, #2a5a3a)' }}
          >
            <div className="flex justify-center">
              <div className="max-w-full rounded-[8px] px-3.5 py-2 text-center" style={css.panel}>
                {caption.show_original && (
                  <p className="text-sm leading-snug opacity-70" style={css.text}>
                    {t('game.previewOriginal')}
                  </p>
                )}
                <p className="text-xl leading-snug" style={css.text}>
                  {t('game.previewText')}
                </p>
              </div>
            </div>
          </div>
          <p className="text-xs text-muted-foreground">{t('game.lookHint')}</p>
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
        {notice && <p className="text-xs text-muted-foreground">{t(`game.notice.${notice}`)}</p>}

        {running && (
          <section className="space-y-2 rounded-lg border border-border p-4">
            <p className="text-xs text-muted-foreground">{t('game.preview')}</p>
            {frame?.source ? (
              <>
                <p className="text-sm whitespace-pre-wrap text-muted-foreground">{frame.source}</p>
                <p className="text-[15px] whitespace-pre-wrap">{frame.translation}</p>
              </>
            ) : (
              <p className="text-sm text-muted-foreground">{isSpeech ? t('game.waitingSpeech') : t('game.waiting')}</p>
            )}
          </section>
        )}

        <p className="text-xs text-muted-foreground">{t('game.waylandHint')}</p>
      </div>
    </div>
  )
}
