import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from 'react'
import i18n from '@/i18n'
import * as api from '@/lib/api'
import { DEFAULT_SHORTCUTS } from '@/lib/shortcuts'

export type Screen = 'text' | 'documents' | 'glossary' | 'history' | 'settings'
export const SCREENS: Screen[] = ['text', 'documents', 'glossary', 'history', 'settings']

interface AppState {
  screen: Screen
  navigate: (s: Screen) => void
  settings: api.Settings | null
  updateSettings: (patch: Partial<api.Settings>) => Promise<void>
  keyStatus: api.KeyStatus
  refreshKey: () => Promise<void>
  online: boolean
  paletteOpen: boolean
  setPaletteOpen: (open: boolean) => void
  /** A history entry to load into the Text screen. */
  textPreset: api.HistoryEntry | null
  openInText: (entry: api.HistoryEntry) => void
}

const Ctx = createContext<AppState | null>(null)

export function applyAppearance(s: api.Settings) {
  document.documentElement.classList.toggle('dark', s.ui.theme === 'dark')
  document.documentElement.lang = s.ui.language
  if (i18n.language !== s.ui.language) void i18n.changeLanguage(s.ui.language)
}

export function AppProvider({ children }: { children: ReactNode }) {
  const [screen, setScreen] = useState<Screen>('text')
  const [settings, setSettings] = useState<api.Settings | null>(null)
  const [keyStatus, setKeyStatus] = useState<api.KeyStatus>({ deepl: null })
  const [online, setOnline] = useState(navigator.onLine)
  const [paletteOpen, setPaletteOpen] = useState(false)
  const [textPreset, setTextPreset] = useState<api.HistoryEntry | null>(null)

  const refreshKey = useCallback(async () => {
    setKeyStatus(await api.keyStatus().catch(() => ({ deepl: null })))
  }, [])

  useEffect(() => {
    void api.getSettings().then((s) => {
      applyAppearance(s)
      setSettings(s)
    })
    void refreshKey()
    const on = () => setOnline(true)
    const off = () => setOnline(false)
    window.addEventListener('online', on)
    window.addEventListener('offline', off)
    return () => {
      window.removeEventListener('online', on)
      window.removeEventListener('offline', off)
    }
  }, [refreshKey])

  const updateSettings = useCallback(
    async (patch: Partial<api.Settings>) => {
      if (!settings) return
      const next = { ...settings, ...patch, ui: { ...settings.ui, ...patch.ui } }
      applyAppearance(next)
      setSettings(next)
      await api.saveSettings(next)
    },
    [settings],
  )

  const openInText = useCallback((entry: api.HistoryEntry) => {
    setTextPreset(entry)
    setScreen('text')
  }, [])

  return (
    <Ctx.Provider
      value={{
        screen,
        navigate: setScreen,
        settings,
        updateSettings,
        keyStatus,
        refreshKey,
        online,
        paletteOpen,
        setPaletteOpen,
        textPreset,
        openInText,
      }}
    >
      {children}
    </Ctx.Provider>
  )
}

/** The current shortcuts, or the defaults before settings load. */
export function useShortcuts(): api.Shortcuts {
  return useApp().settings?.ui.shortcuts ?? DEFAULT_SHORTCUTS
}

export function useApp(): AppState {
  const v = useContext(Ctx)
  if (!v) throw new Error('useApp outside AppProvider')
  return v
}
