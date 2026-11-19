import { BookA, Download, Pencil, Plus, Search, Trash2, Upload } from 'lucide-react'
import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { EmptyState, ErrorNotice } from '@/components/common/states'
import { LanguageSelect } from '@/components/common/selects'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import * as api from '@/lib/api'
import { languageName } from '@/lib/languages'
import { cn } from '@/lib/utils'

type Editing = { id?: number; source: string; target: string } | null

export function GlossaryScreen({ active }: { active: boolean }) {
  const { t, i18n } = useTranslation()
  // Glossaries are per base language (TR → EN), so both sides use source codes.
  const [sourceLang, setSourceLang] = useState('TR')
  const [targetLang, setTargetLang] = useState('EN')
  const [pairs, setPairs] = useState<api.GlossaryPair[]>([])
  const [terms, setTerms] = useState<api.Term[]>([])
  const [query, setQuery] = useState('')
  const [editing, setEditing] = useState<Editing>(null)
  const [error, setError] = useState<api.ApiError | null>(null)
  const [notice, setNotice] = useState('')

  const reload = useCallback(async () => {
    try {
      const [p, ts] = await Promise.all([api.listGlossaryPairs(), api.listTerms(sourceLang, targetLang)])
      setPairs(p)
      setTerms(ts)
    } catch (e) {
      setError(e as api.ApiError)
    }
  }, [sourceLang, targetLang])

  useEffect(() => {
    if (active) void reload()
  }, [active, reload])

  const flash = (msg: string) => {
    setNotice(msg)
    setTimeout(() => setNotice(''), 2500)
  }

  const run = (fn: () => Promise<void>) => async () => {
    setError(null)
    try {
      await fn()
    } catch (e) {
      setError(e as api.ApiError)
    }
  }

  const save = run(async () => {
    if (!editing || !editing.source.trim() || !editing.target.trim()) return
    await api.upsertTerm({ ...editing, sourceLang, targetLang })
    setEditing(null)
    await reload()
  })

  const doImport = run(async () => {
    const n = await api.importTermsCsv(sourceLang, targetLang)
    if (n === null) return
    await reload()
    flash(t('glossary.imported', { count: n }))
  })

  const doExport = run(async () => {
    const n = await api.exportTermsCsv(sourceLang, targetLang)
    if (n !== null) flash(t('glossary.exported', { count: n }))
  })

  const q = query.trim().toLocaleLowerCase()
  const shown = q ? terms.filter((x) => `${x.source} ${x.target}`.toLocaleLowerCase().includes(q)) : terms

  return (
    <div className="flex h-full flex-col">
      <header className="flex flex-wrap items-center gap-2 border-b border-border px-5 py-3">
        <LanguageSelect kind="source" value={sourceLang} onChange={(v) => v && setSourceLang(v)} className="w-40" label={t('glossary.pair')} />
        <span className="text-muted-foreground">→</span>
        <LanguageSelect kind="source" value={targetLang} onChange={(v) => v && setTargetLang(v)} className="w-40" label={t('glossary.pair')} />
        <div className="ml-auto flex items-center gap-1">
          {notice && <span className="mr-2 text-xs text-muted-foreground">{notice}</span>}
          <Button variant="ghost" size="sm" onClick={() => void doImport()}>
            <Upload />
            {t('glossary.import')}
          </Button>
          <Button variant="ghost" size="sm" onClick={() => void doExport()} disabled={!terms.length}>
            <Download />
            {t('glossary.export')}
          </Button>
          <Button size="sm" onClick={() => setEditing({ source: '', target: '' })}>
            <Plus />
            {t('glossary.addTerm')}
          </Button>
        </div>
      </header>

      {pairs.length > 0 && (
        <nav className="flex flex-wrap gap-1.5 border-b border-border px-5 py-2" aria-label={t('glossary.pairs')}>
          {pairs.map((p) => {
            const current = p.source_lang === sourceLang && p.target_lang === targetLang
            return (
              <button
                key={`${p.source_lang}-${p.target_lang}`}
                onClick={() => {
                  setSourceLang(p.source_lang)
                  setTargetLang(p.target_lang)
                }}
                className={cn(
                  'rounded-md border border-border px-2 py-0.5 text-xs text-muted-foreground hover:text-foreground',
                  current && 'border-primary text-foreground',
                )}
              >
                {languageName(p.source_lang, i18n.language)} → {languageName(p.target_lang, i18n.language)} · {p.count}
              </button>
            )
          })}
        </nav>
      )}

      <div className="min-h-0 flex-1 space-y-3 overflow-auto p-5">
        {error && <ErrorNotice error={error} />}
        {terms.length === 0 ? (
          <EmptyState icon={BookA} title={t('glossary.empty')} hint={t('glossary.emptyHint')} className="pt-12" />
        ) : (
          <>
            <div className="relative max-w-xs">
              <Search className="absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
              <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder={t('common.search')} className="pl-8" />
            </div>
            {shown.length === 0 ? (
              <EmptyState icon={Search} title={t('glossary.noResults')} />
            ) : (
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>{t('glossary.source')}</TableHead>
                    <TableHead>{t('glossary.target')}</TableHead>
                    <TableHead className="w-20" />
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {shown.map((x) => (
                    <TableRow key={x.id}>
                      <TableCell>{x.source}</TableCell>
                      <TableCell>{x.target}</TableCell>
                      <TableCell>
                        <div className="flex justify-end gap-0.5">
                          <Button variant="ghost" size="icon-sm" aria-label={t('common.edit')} onClick={() => setEditing({ id: x.id, source: x.source, target: x.target })}>
                            <Pencil />
                          </Button>
                          <Button
                            variant="ghost"
                            size="icon-sm"
                            aria-label={t('common.delete')}
                            onClick={run(async () => {
                              await api.deleteTerm(x.id)
                              await reload()
                            })}
                          >
                            <Trash2 />
                          </Button>
                        </div>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            )}
          </>
        )}
      </div>

      <Dialog open={!!editing} onOpenChange={(o) => !o && setEditing(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{editing?.id ? t('glossary.editTerm') : t('glossary.addTerm')}</DialogTitle>
            <DialogDescription>
              {languageName(sourceLang, i18n.language)} → {languageName(targetLang, i18n.language)}
            </DialogDescription>
          </DialogHeader>
          <form
            className="grid gap-3"
            onSubmit={(e) => {
              e.preventDefault()
              void save()
            }}
          >
            <div className="grid gap-1.5">
              <Label htmlFor="term-src">{t('glossary.source')}</Label>
              <Input id="term-src" autoFocus value={editing?.source ?? ''} onChange={(e) => setEditing((s) => s && { ...s, source: e.target.value })} />
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor="term-tgt">{t('glossary.target')}</Label>
              <Input id="term-tgt" value={editing?.target ?? ''} onChange={(e) => setEditing((s) => s && { ...s, target: e.target.value })} />
            </div>
            <DialogFooter>
              <Button type="button" variant="ghost" onClick={() => setEditing(null)}>
                {t('common.cancel')}
              </Button>
              <Button type="submit">{t('common.save')}</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  )
}
