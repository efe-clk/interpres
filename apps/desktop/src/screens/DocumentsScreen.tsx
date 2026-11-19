import { ExternalLink, FileText, FolderOpen, Inbox, Plus, Trash2, Upload, X } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { EngineSelect, LanguageSelect } from '@/components/common/selects'
import { EmptyState, ErrorNotice, Kbd } from '@/components/common/states'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Progress } from '@/components/ui/progress'
import { Label } from '@/components/ui/label'
import { Switch } from '@/components/ui/switch'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { useHotkey } from '@/hooks/use-hotkey'
import * as api from '@/lib/api'
import { useApp } from '@/lib/app-context'
import { languageName } from '@/lib/languages'
import { cn } from '@/lib/utils'

const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path
const ext = (name: string) => name.split('.').pop()?.toLowerCase() ?? ''
const RUNNING: api.JobStatus[] = ['queued', 'uploading', 'translating', 'downloading']

export function DocumentsScreen({ active }: { active: boolean }) {
  const { t, i18n } = useTranslation()
  const { settings } = useApp()
  const [jobs, setJobs] = useState<api.DocumentJob[]>([])
  const [source, setSource] = useState<string | null>(settings?.source_lang ?? null)
  const [target, setTarget] = useState(settings?.target_lang ?? 'EN-US')
  const [engine, setEngine] = useState<api.EngineId>('deepl')
  const [useGlossary, setUseGlossary] = useState(true)
  const [hover, setHover] = useState(false)
  const [rejected, setRejected] = useState<string[]>([])
  const [error, setError] = useState<api.ApiError | null>(null)
  const opts = useRef({ target, engine, useGlossary, source })
  opts.current = { target, engine, useGlossary, source }

  useEffect(() => api.subscribeJobs(setJobs), [])

  const add = async (paths: string[]) => {
    const ok = paths.filter((p) => api.DOCUMENT_EXTENSIONS.includes(ext(p)))
    setRejected(paths.filter((p) => !ok.includes(p)).map(fileName))
    setError(null)
    if (!ok.length) return
    const { target, engine, useGlossary, source } = opts.current
    try {
      await api.addDocuments(ok, {
        engine,
        useGlossary: useGlossary && !!source,
        request: { source, target, formality: 'default' },
      })
    } catch (e) {
      setError(e as api.ApiError)
    }
  }
  const pick = async () => void add(await api.pickDocuments())

  useHotkey('mod+o', () => void pick(), active)

  const addRef = useRef(add)
  addRef.current = add
  useEffect(() => {
    let off = () => {}
    let stopped = false
    void api
      .onFileDrop(setHover, (paths) => void addRef.current(paths))
      .then((f) => (stopped ? f() : (off = f)))
    return () => {
      stopped = true
      off()
    }
  }, [])

  const act = (fn: () => Promise<void>) => () => void fn().catch((e) => setError(e as api.ApiError))

  return (
    <div className="flex h-full flex-col">
      <header className="flex flex-wrap items-center gap-2 border-b border-border px-5 py-3">
        <LanguageSelect kind="source" value={source} onChange={setSource} className="w-40" label={t('documents.source')} />
        <span className="text-muted-foreground">→</span>
        <LanguageSelect kind="target" value={target} onChange={(v) => v && setTarget(v)} className="w-40" label={t('documents.target')} />
        <EngineSelect value={engine} onChange={setEngine} className="w-32" label={t('documents.engine')} />
        <div className="flex items-center gap-2 pl-1" title={source ? undefined : t('glossary.sourceNeeded')}>
          <Switch id="use-glossary" checked={useGlossary && !!source} onCheckedChange={setUseGlossary} disabled={!source} />
          <Label htmlFor="use-glossary" className="text-sm font-normal text-muted-foreground">
            {t('documents.useGlossary')}
          </Label>
        </div>
        <Button size="sm" className="ml-auto" onClick={() => void pick()}>
          <Plus />
          {t('documents.add')}
          <span className="ml-1 font-mono text-[11px] opacity-70">Ctrl O</span>
        </Button>
      </header>

      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-auto p-5">
        <div
          onDragOver={(e) => {
            e.preventDefault()
            setHover(true)
          }}
          onDragLeave={() => setHover(false)}
          onDrop={(e) => {
            e.preventDefault()
            setHover(false)
            void add(Array.from(e.dataTransfer.files).map((f) => f.name))
          }}
          onClick={() => void pick()}
          className={cn(
            'flex cursor-pointer flex-col items-center justify-center gap-1.5 rounded-lg border border-dashed border-border px-6 py-8 text-center transition-colors hover:bg-muted/30',
            hover && 'border-primary bg-primary/5',
          )}
        >
          <Upload className="size-5 text-muted-foreground" strokeWidth={1.5} />
          <p className="text-sm font-medium">{t('documents.drop')}</p>
          <p className="text-xs text-muted-foreground">{t('documents.dropHint')}</p>
          {rejected.length > 0 && (
            <p className="mt-1 text-xs text-destructive">{rejected.map((n) => t('documents.unsupported', { name: n })).join(' · ')}</p>
          )}
        </div>

        {error && <ErrorNotice error={error} />}

        {jobs.length === 0 ? (
          <EmptyState icon={Inbox} title={t('documents.empty')} hint={t('documents.emptyHint')}>
            <Kbd>Ctrl O</Kbd>
          </EmptyState>
        ) : (
          <Table className="table-fixed">
            <TableHeader>
              <TableRow>
                <TableHead>{t('documents.file')}</TableHead>
                <TableHead className="w-32">{t('documents.target')}</TableHead>
                <TableHead className="w-24">{t('documents.engine')}</TableHead>
                <TableHead className="w-44">{t('documents.state')}</TableHead>
                <TableHead className="w-28" />
              </TableRow>
            </TableHeader>
            <TableBody>
              {jobs.map((j) => (
                <TableRow key={j.id}>
                  <TableCell>
                    <div className="flex min-w-0 items-center gap-2" title={j.input}>
                      <FileText className="size-4 shrink-0 text-muted-foreground" strokeWidth={1.5} />
                      <span className="truncate">{j.name}</span>
                    </div>
                    {j.glossary && <span className="ml-6 text-xs text-muted-foreground">{j.glossary}</span>}
                  </TableCell>
                  <TableCell className="truncate text-muted-foreground">{languageName(j.request.target, i18n.language)}</TableCell>
                  <TableCell className="text-muted-foreground">{t(`engine.${j.engine}`)}</TableCell>
                  <TableCell>
                    <JobState job={j} />
                  </TableCell>
                  <TableCell>
                    <div className="flex justify-end gap-0.5">
                      {j.status === 'done' && (
                        <>
                          <IconAction label={t('documents.reveal')} onClick={act(() => api.revealDocument(j.id))}>
                            <FolderOpen />
                          </IconAction>
                          <IconAction label={t('documents.open')} onClick={act(() => api.openDocument(j.id))}>
                            <ExternalLink />
                          </IconAction>
                        </>
                      )}
                      {RUNNING.includes(j.status) ? (
                        <IconAction label={t('documents.cancel')} onClick={act(() => api.cancelDocument(j.id))}>
                          <X />
                        </IconAction>
                      ) : (
                        <IconAction label={t('documents.remove')} onClick={act(() => api.removeDocument(j.id))}>
                          <Trash2 />
                        </IconAction>
                      )}
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </div>
    </div>
  )
}

function JobState({ job }: { job: api.DocumentJob }) {
  const { t } = useTranslation()
  if (job.status === 'error') {
    const title = job.error ? t(`errors.${job.error.kind}.title`) : t('documents.status.error')
    return (
      <Tooltip>
        <TooltipTrigger asChild>
          <Badge variant="destructive" className="max-w-full truncate">
            {title}
          </Badge>
        </TooltipTrigger>
        <TooltipContent className="max-w-xs">{job.error?.message || t(`errors.${job.error?.kind ?? 'service'}.hint`) || title}</TooltipContent>
      </Tooltip>
    )
  }
  if (job.status === 'done') return <Badge variant="secondary">{t('documents.status.done')}</Badge>
  if (job.status === 'cancelled') return <Badge variant="outline">{t('documents.status.cancelled')}</Badge>
  return (
    <div className="space-y-1">
      <span className="text-xs text-muted-foreground">
        {t(`documents.status.${job.status}`)} · {job.progress}%
      </span>
      <Progress value={job.progress} className="h-1" />
    </div>
  )
}

function IconAction({ label, onClick, children }: { label: string; onClick: () => void; children: React.ReactNode }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant="ghost" size="icon-sm" onClick={onClick} aria-label={label}>
          {children}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  )
}
