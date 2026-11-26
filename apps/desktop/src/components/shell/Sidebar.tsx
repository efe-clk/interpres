import { BookA, FileText, History, Languages, Radio, Settings, type LucideIcon } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { useApp, type Screen } from '@/lib/app-context'
import { cn } from '@/lib/utils'

const ITEMS: { screen: Screen; icon: LucideIcon; key: string }[] = [
  { screen: 'text', icon: Languages, key: '1' },
  { screen: 'documents', icon: FileText, key: '2' },
  { screen: 'glossary', icon: BookA, key: '3' },
  { screen: 'history', icon: History, key: '4' },
  { screen: 'game', icon: Radio, key: '5' },
  { screen: 'settings', icon: Settings, key: '6' },
]

export function Sidebar() {
  const { t } = useTranslation()
  const { screen, navigate, settings, keyStatus, online } = useApp()

  const engine = settings?.engine ?? 'deepl'
  const status = !online
    ? { label: t('status.offline'), tone: 'bg-destructive' }
    : !keyStatus.deepl
      ? { label: t('status.noKey'), tone: 'bg-amber-500' }
      : { label: t(`status.${keyStatus.deepl}`), tone: 'bg-emerald-500' }

  return (
    <aside className="flex w-52 shrink-0 flex-col border-r border-border bg-sidebar">
      <div className="px-4 pt-5 pb-4 text-sm font-semibold tracking-tight">Interpres</div>
      <nav className="flex flex-col gap-0.5 px-2" aria-label="main">
        {ITEMS.map(({ screen: s, icon: Icon, key }) => (
          <button
            key={s}
            onClick={() => navigate(s)}
            aria-current={screen === s ? 'page' : undefined}
            className={cn(
              'group flex items-center gap-2.5 rounded-md px-2.5 py-1.5 text-sm text-muted-foreground transition-colors',
              'hover:bg-sidebar-accent hover:text-foreground',
              screen === s && 'bg-sidebar-accent text-foreground',
            )}
          >
            <Icon className={cn('size-4', screen === s && 'text-primary')} strokeWidth={1.75} />
            <span className="flex-1 text-left">{t(`nav.${s}`)}</span>
            <span className="font-mono text-[11px] opacity-0 transition-opacity group-hover:opacity-60">
              Ctrl {key}
            </span>
          </button>
        ))}
      </nav>
      <div className="mt-auto border-t border-border px-4 py-3">
        <button
          onClick={() => navigate('settings')}
          className="flex w-full items-center gap-2 text-left text-xs text-muted-foreground hover:text-foreground"
        >
          <span className={cn('size-1.5 rounded-full', status.tone)} />
          <span className="text-foreground">{t(`engine.${engine}`)}</span>
          <span>·</span>
          <span>{status.label}</span>
        </button>
      </div>
    </aside>
  )
}
