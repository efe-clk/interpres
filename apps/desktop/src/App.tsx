import { useEffect } from 'react'
import { CommandPalette } from '@/components/shell/CommandPalette'
import { Sidebar } from '@/components/shell/Sidebar'
import { TooltipProvider } from '@/components/ui/tooltip'
import { useHotkey } from '@/hooks/use-hotkey'
import { AppProvider, SCREENS, useApp, useShortcuts } from '@/lib/app-context'
import { cn } from '@/lib/utils'
import { DocumentsScreen } from '@/screens/DocumentsScreen'
import { GameScreen } from '@/screens/GameScreen'
import { GlossaryScreen } from '@/screens/GlossaryScreen'
import { HistoryScreen } from '@/screens/HistoryScreen'
import { SettingsScreen } from '@/screens/SettingsScreen'
import { TextScreen } from '@/screens/TextScreen'

function Shell() {
  const { screen, navigate, settings, setPaletteOpen, paletteOpen } = useApp()
  const shortcuts = useShortcuts()

  useHotkey(shortcuts.palette, () => setPaletteOpen(!paletteOpen))
  useHotkey('mod+,', () => navigate('settings'))
  useEffect(() => {
    // Ctrl+1..5 switch screens.
    const onKey = (e: KeyboardEvent) => {
      const i = Number(e.key) - 1
      if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && SCREENS[i]) {
        e.preventDefault()
        navigate(SCREENS[i])
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [navigate])

  if (!settings) return <div className="h-screen bg-background" />

  // Screens stay mounted so their state survives navigation.
  const pane = (s: typeof screen) => cn('h-full', screen !== s && 'hidden')
  return (
    <div className="flex h-screen overflow-hidden bg-background text-foreground">
      <Sidebar />
      <main className="min-w-0 flex-1">
        <div className={pane('text')}>
          <TextScreen active={screen === 'text'} />
        </div>
        <div className={pane('documents')}>
          <DocumentsScreen active={screen === 'documents'} />
        </div>
        <div className={pane('glossary')}>
          <GlossaryScreen active={screen === 'glossary'} />
        </div>
        <div className={pane('history')}>
          <HistoryScreen active={screen === 'history'} />
        </div>
        <div className={pane('game')}>
          <GameScreen active={screen === 'game'} />
        </div>
        <div className={pane('settings')}>
          <SettingsScreen />
        </div>
      </main>
      <CommandPalette />
    </div>
  )
}

export default function App() {
  return (
    <TooltipProvider delayDuration={400}>
      <AppProvider>
        <Shell />
      </AppProvider>
    </TooltipProvider>
  )
}
