import { History, Search } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { EmptyState } from '@/components/common/states'
import { Input } from '@/components/ui/input'
import * as api from '@/lib/api'
import { useApp } from '@/lib/app-context'
import { languageName } from '@/lib/languages'

export function HistoryScreen() {
  const { t, i18n } = useTranslation()
  const { openInText } = useApp()
  const [query, setQuery] = useState('')
  const [items, setItems] = useState<api.HistoryEntry[] | null>(null)

  useEffect(() => {
    void api.listHistory(query).then(setItems)
  }, [query])

  const fmt = new Intl.DateTimeFormat(i18n.language, { dateStyle: 'medium', timeStyle: 'short' })

  return (
    <div className="flex h-full flex-col">
      <header className="border-b border-border px-5 py-3">
        <div className="relative max-w-sm">
          <Search className="absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder={t('history.search')} className="pl-8" />
        </div>
      </header>
      <div className="min-h-0 flex-1 overflow-auto p-3">
        {items === null ? null : items.length === 0 ? (
          query ? (
            <EmptyState icon={Search} title={t('history.noResults')} className="pt-16" />
          ) : (
            <EmptyState icon={History} title={t('history.empty')} hint={t('history.emptyHint')} className="pt-16" />
          )
        ) : (
          <ul className="flex flex-col">
            {items.map((h) => (
              <li key={h.id}>
                <button
                  onClick={() => openInText(h)}
                  title={t('history.reopen')}
                  className="grid w-full grid-cols-2 gap-4 rounded-md px-3 py-2.5 text-left text-sm hover:bg-muted/60"
                >
                  <span className="min-w-0">
                    <span className="block truncate">{h.sourceText}</span>
                    <span className="mt-0.5 block text-xs text-muted-foreground">
                      {h.source ? languageName(h.source, i18n.language) : t('text.auto')} → {languageName(h.target, i18n.language)} · {t(`engine.${h.engine}`)}
                    </span>
                  </span>
                  <span className="min-w-0">
                    <span className="block truncate text-muted-foreground">{h.targetText}</span>
                    <span className="mt-0.5 block text-xs text-muted-foreground">{fmt.format(new Date(h.createdAt))}</span>
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  )
}
