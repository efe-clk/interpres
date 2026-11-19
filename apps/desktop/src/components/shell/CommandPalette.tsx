import { ArrowLeftRight, Languages, Moon, Play } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandShortcut,
} from '@/components/ui/command'
import { SCREENS, useApp } from '@/lib/app-context'

/** Actions the Text screen listens for. */
export const TEXT_EVENTS = { translate: 'interpres:translate', swap: 'interpres:swap' } as const

export function CommandPalette() {
  const { t } = useTranslation()
  const { paletteOpen, setPaletteOpen, navigate, settings, updateSettings } = useApp()

  const run = (fn: () => void) => {
    setPaletteOpen(false)
    fn()
  }
  const textAction = (event: string) => () => {
    navigate('text')
    setTimeout(() => window.dispatchEvent(new Event(event)), 0)
  }

  return (
    <CommandDialog
      open={paletteOpen}
      onOpenChange={setPaletteOpen}
      title={t('shortcuts.palette')}
      description={t('palette.placeholder')}
    >
      <Command>
      <CommandInput placeholder={t('palette.placeholder')} />
      <CommandList>
        <CommandEmpty>{t('palette.empty')}</CommandEmpty>
        <CommandGroup heading={t('palette.navigation')}>
          {SCREENS.map((s, i) => (
            <CommandItem key={s} onSelect={() => run(() => navigate(s))}>
              {t(`nav.${s}`)}
              <CommandShortcut>Ctrl {i + 1}</CommandShortcut>
            </CommandItem>
          ))}
        </CommandGroup>
        <CommandGroup heading={t('palette.actions')}>
          <CommandItem onSelect={() => run(textAction(TEXT_EVENTS.translate))}>
            <Play />
            {t('shortcuts.translate')}
            <CommandShortcut>Ctrl Enter</CommandShortcut>
          </CommandItem>
          <CommandItem onSelect={() => run(textAction(TEXT_EVENTS.swap))}>
            <ArrowLeftRight />
            {t('shortcuts.swap')}
            <CommandShortcut>Ctrl Shift S</CommandShortcut>
          </CommandItem>
          <CommandItem
            onSelect={() =>
              run(() => settings && updateSettings({ ui: { ...settings.ui, theme: settings.ui.theme === 'dark' ? 'light' : 'dark' } }))
            }
          >
            <Moon />
            {t('palette.toggleTheme')}
          </CommandItem>
          <CommandItem
            onSelect={() =>
              run(() => settings && updateSettings({ ui: { ...settings.ui, language: settings.ui.language === 'tr' ? 'en' : 'tr' } }))
            }
          >
            <Languages />
            {t('palette.toggleLanguage')}
          </CommandItem>
        </CommandGroup>
      </CommandList>
      </Command>
    </CommandDialog>
  )
}
