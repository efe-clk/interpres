import { BookA, Download, Pencil, Plus, Search, Trash2, Upload } from 'lucide-react'
import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { LanguageSelect } from '@/components/common/selects'
import { EmptyState } from '@/components/common/states'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { Textarea } from '@/components/ui/textarea'
import * as api from '@/lib/api'

type Editing = { id?: string; source: string; target: string } | null

export function GlossaryScreen() {
  const { t } = useTranslation()
  const [sourceLang, setSourceLang] = useState('TR')
  const [targetLang, setTargetLang] = useState('EN-US')
  const [terms, setTerms] = useState<api.GlossaryEntry[]>([])
  const [query, setQuery] = useState('')
  const [editing, setEditing] = useState<Editing>(null)
  const [importOpen, setImportOpen] = useState(false)
  const [csv, setCsv] = useState('')
  const [notice, setNotice] = useState('')

  const reload = useCallback(async () => setTerms(await api.listTerms(sourceLang, targetLang)), [sourceLang, targetLang])
  useEffect(() => {
    void reload()
  }, [reload])

  const flash = (msg: string) => {
    setNotice(msg)
    setTimeout(() => setNotice(''), 2000)
  }

  const save = async () => {
    if (!editing || !editing.source.trim() || !editing.target.trim()) return
    await api.upsertTerm({ ...editing, source: editing.source.trim(), target: editing.target.trim(), sourceLang, targetLang })
    setEditing(null)
    await reload()
  }

  const doImport = async () => {
    const n = await api.importTermsCsv(csv, sourceLang, targetLang)
    setImportOpen(false)
    setCsv('')
    await reload()
    flash(t('glossary.imported', { count: n }))
  }

  const doExport = async () => {
    await api.writeClipboard(await api.exportTermsCsv(sourceLang, targetLang))
    flash(t('glossary.exported'))
  }

  const q = query.trim().toLocaleLowerCase()
  const shown = q ? terms.filter((x) => `${x.source} ${x.target}`.toLocaleLowerCase().includes(q)) : terms

  return (
    <div className="flex h-full flex-col">
      <header className="flex flex-wrap items-center gap-2 border-b border-border px-5 py-3">
        <LanguageSelect kind="source" value={sourceLang} onChange={(v) => v && setSourceLang(v)} className="w-48" label={t('glossary.pair')} />
        <span className="text-muted-foreground">→</span>
        <LanguageSelect kind="target" value={targetLang} onChange={(v) => v && setTargetLang(v)} className="w-48" label={t('glossary.pair')} />
        <div className="ml-auto flex items-center gap-1">
          {notice && <span className="mr-2 text-xs text-muted-foreground">{notice}</span>}
          <Button variant="ghost" size="sm" onClick={() => setImportOpen(true)}>
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

      <div className="min-h-0 flex-1 overflow-auto p-5">
        {terms.length === 0 ? (
          <EmptyState icon={BookA} title={t('glossary.empty')} hint={t('glossary.emptyHint')} className="pt-16" />
        ) : (
          <>
            <div className="relative mb-3 max-w-xs">
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
                            onClick={async () => {
                              await api.deleteTerm(x.id)
                              await reload()
                            }}
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
              {sourceLang} → {targetLang}
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

      <Dialog open={importOpen} onOpenChange={setImportOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t('glossary.import')}</DialogTitle>
            <DialogDescription>{t('glossary.csvFormat')}</DialogDescription>
          </DialogHeader>
          <Textarea value={csv} onChange={(e) => setCsv(e.target.value)} rows={8} className="font-mono text-xs" placeholder="kaynak,source" />
          <DialogFooter>
            <Button variant="ghost" onClick={() => setImportOpen(false)}>
              {t('common.cancel')}
            </Button>
            <Button onClick={() => void doImport()} disabled={!csv.trim()}>
              {t('glossary.import')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
