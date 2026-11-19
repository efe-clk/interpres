import { useEffect, useRef } from 'react'

/**
 * Binds a keyboard shortcut such as `mod+k`, `mod+enter`, `mod+shift+s`, `escape`.
 * `mod` is Ctrl (or Cmd on macOS).
 */
export function useHotkey(combo: string, handler: (e: KeyboardEvent) => void, enabled = true) {
  const ref = useRef(handler)
  ref.current = handler

  useEffect(() => {
    if (!enabled) return
    const parts = combo.toLowerCase().split('+')
    const key = parts[parts.length - 1]
    const mod = parts.includes('mod')
    const shift = parts.includes('shift')
    const alt = parts.includes('alt')
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) !== mod || e.shiftKey !== shift || e.altKey !== alt) return
      if (e.key.toLowerCase() !== key) return
      e.preventDefault()
      ref.current(e)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [combo, enabled])
}
