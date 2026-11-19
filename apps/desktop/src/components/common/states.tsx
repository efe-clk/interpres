import type { LucideIcon } from 'lucide-react'
import { AlertCircle } from 'lucide-react'
import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import type { ApiError } from '@/lib/api'
import { useApp } from '@/lib/app-context'
import { cn } from '@/lib/utils'

export function EmptyState({
  icon: Icon,
  title,
  hint,
  children,
  className,
}: {
  icon: LucideIcon
  title: string
  hint?: string
  children?: ReactNode
  className?: string
}) {
  return (
    <div className={cn('flex flex-col items-center justify-center gap-2 p-8 text-center', className)}>
      <Icon className="size-6 text-muted-foreground" strokeWidth={1.5} />
      <p className="text-sm font-medium">{title}</p>
      {hint && <p className="max-w-xs text-sm text-muted-foreground">{hint}</p>}
      {children && <div className="mt-2 flex gap-2">{children}</div>}
    </div>
  )
}

export function ErrorNotice({ error, onRetry }: { error: ApiError; onRetry?: () => void }) {
  const { t } = useTranslation()
  const { navigate } = useApp()
  const keyProblem = error.kind === 'missing_key' || error.kind === 'invalid_key'
  const hint = t(`errors.${error.kind}.hint`) || error.message
  return (
    <div role="alert" className="flex items-start gap-3 rounded-lg border border-destructive/30 bg-destructive/5 p-3">
      <AlertCircle className="mt-0.5 size-4 shrink-0 text-destructive" />
      <div className="min-w-0 flex-1 space-y-0.5">
        <p className="text-sm font-medium">{t(`errors.${error.kind}.title`)}</p>
        {hint && <p className="text-sm break-words text-muted-foreground">{hint}</p>}
      </div>
      {keyProblem ? (
        <Button size="sm" variant="outline" onClick={() => navigate('settings')}>
          {t('common.goToSettings')}
        </Button>
      ) : (
        onRetry && (
          <Button size="sm" variant="outline" onClick={onRetry}>
            {t('common.retry')}
          </Button>
        )
      )}
    </div>
  )
}

export function Kbd({ children }: { children: ReactNode }) {
  return (
    <kbd className="rounded border border-border bg-muted px-1.5 py-0.5 font-mono text-[11px] text-muted-foreground">
      {children}
    </kbd>
  )
}
