import { ExternalLink, FileText, FolderOpen, Inbox, Trash2, Upload } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { EngineSelect, LanguageSelect } from '@/components/common/selects'
import { EmptyState } from '@/components/common/states'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Progress } from '@/components/ui/progress'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import * as api from '@/lib/api'
import { useApp } from '@/lib/app-context'
import { languageName } from '@/lib/languages'
import { cn } from '@/lib/utils'

const NO_GLOSSARY = '__none'
const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path
const ext = (name: string) => name.split('.').pop()?.toLowerCase() ?? ''

export function DocumentsScreen() {
  const { t, i18n } = useTranslation()
  const { settings } = useApp()
  const [jobs, setJobs] = useState<api.DocumentJob[]>([])
  const [glossaries, setGlossaries] = useState<api.Glossary[]>([])
  const [target, setTarget] = useState(settings?.target_lang ?? 'EN-US')
  const [engine, setEngine] = useState<api.EngineId>('deepl')
  const [glossary, setGlossary] = useState<string | null>(null)
  const [hover, setHover] = useState(false)
  const [rejected, setRejected] = useState<string[]>([])
  const opts = useRef({ target, engine, glossary })
  opts.current = { target, engine, glossary }

  useEffect(() => api.subscribeJobs(setJobs), [])
  useEffect(() => {
    void api.listGlossaries().then(setGlossaries)
  }, [])

  const add = (paths: string[]) => {
    const names = paths.map(fileName)
    const ok = names.filter((n) => api.DOCUMENT_EXTENSIONS.includes(ext(n)))
    setRejected(names.filter((n) => !ok.includes(n)))
    if (ok.length) void api.addDocuments(ok, opts.current)
  }

  useEffect(() => {
    let off = () => {}
    void api.onFileDrop(setHover, add).then((f) => (off = f))
    return () => off()
  }, [])

  return (
    <div className="flex h-full flex-col">
      <header className="flex flex-wrap items-center gap-2 border-b border-border px-5 py-3">
        <LanguageSelect kind="target" value={target} onChange={(v) => v && setTarget(v)} className="w-44" label={t('documents.target')} />
        <EngineSelect value={engine} onChange={setEngine} className="w-32" label={t('documents.engine')} />
        <Select value={glossary ?? NO_GLOSSARY} onValueChange={(v) => setGlossary(v === NO_GLOSSARY ? null : v)}>
          <SelectTrigger className="w-40" aria-label={t('documents.glossary')}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent position="popper">
            <SelectItem value={NO_GLOSSARY}>{t('documents.noGlossary')}</SelectItem>
            {glossaries.map((g) => (
              <SelectItem key={g.id} value={g.name}>
                {g.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
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
            add(Array.from(e.dataTransfer.files).map((f) => f.name))
          }}
          className={cn(
            'flex flex-col items-center justify-center gap-1.5 rounded-lg border border-dashed border-border px-6 py-8 text-center transition-colors',
            hover && 'border-primary bg-primary/5',
          )}
        >
          <Upload className="size-5 text-muted-foreground" strokeWidth={1.5} />
          <p className="text-sm font-medium">{t('documents.drop')}</p>
          <p className="text-xs text-muted-foreground">{t('documents.dropHint')}</p>
          <Button variant="outline" size="sm" className="mt-2" onClick={() => add(['ornek-sozlesme.docx'])}>
            {t('documents.addSample')}
          </Button>
          {rejected.length > 0 && (
            <p className="mt-1 text-xs text-destructive">{rejected.map((n) => t('documents.unsupported', { name: n })).join(' · ')}</p>
          )}
        </div>

        {jobs.length === 0 ? (
          <EmptyState icon={Inbox} title={t('documents.empty')} hint={t('documents.emptyHint')} />
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t('documents.file')}</TableHead>
                <TableHead className="w-28">{t('documents.target')}</TableHead>
                <TableHead className="w-24">{t('documents.engine')}</TableHead>
                <TableHead className="w-44">{t('documents.state')}</TableHead>
                <TableHead className="w-24" />
              </TableRow>
            </TableHeader>
            <TableBody>
              {jobs.map((j) => (
                <TableRow key={j.id}>
                  <TableCell>
                    <div className="flex min-w-0 items-center gap-2">
                      <FileText className="size-4 shrink-0 text-muted-foreground" strokeWidth={1.5} />
                      <span className="truncate">{j.name}</span>
                    </div>
                    {j.glossary && <span className="ml-6 text-xs text-muted-foreground">{j.glossary}</span>}
                  </TableCell>
                  <TableCell className="text-muted-foreground">{languageName(j.target, i18n.language)}</TableCell>
                  <TableCell className="text-muted-foreground">{t(`engine.${j.engine}`)}</TableCell>
                  <TableCell>
                    {j.status === 'error' ? (
                      <Badge variant="destructive">{j.error ? t(`errors.${j.error}.title`) : t('documents.status.error')}</Badge>
                    ) : j.status === 'done' ? (
                      <Badge variant="secondary">{t('documents.status.done')}</Badge>
                    ) : (
                      <div className="space-y-1">
                        <span className="text-xs text-muted-foreground">
                          {t(`documents.status.${j.status}`)} · {j.progress}%
                        </span>
                        <Progress value={j.progress} className="h-1" />
                      </div>
                    )}
                  </TableCell>
                  <TableCell>
                    <div className="flex justify-end gap-0.5">
                      {j.status === 'done' && (
                        <>
                          <IconAction label={t('documents.reveal')} onClick={() => void api.revealDocument(j.id)}>
                            <FolderOpen />
                          </IconAction>
                          <IconAction label={t('documents.open')} onClick={() => void api.openDocument(j.id)}>
                            <ExternalLink />
                          </IconAction>
                        </>
                      )}
                      <IconAction label={t('documents.remove')} onClick={() => void api.removeDocument(j.id)}>
                        <Trash2 />
                      </IconAction>
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
