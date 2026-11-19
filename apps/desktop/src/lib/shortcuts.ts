// Shortcut combos are stored as `mod+shift+k` (`mod` is Ctrl, or Cmd on macOS).

import type { Shortcuts } from '@/lib/api'

export const DEFAULT_SHORTCUTS: Shortcuts = {
  palette: 'mod+k',
  translate: 'mod+enter',
  swap: 'mod+shift+s',
  copy: 'mod+shift+c',
  add_files: 'mod+o',
  quick: 'mod+alt+t',
}

const MODIFIERS = new Set(['Control', 'Shift', 'Alt', 'Meta', 'AltGraph'])

/** The key part of a combo for a keyboard event: `k`, `enter`, `space`, `f5`… */
export function eventKey(e: KeyboardEvent): string {
  return e.key === ' ' ? 'space' : e.key.toLowerCase()
}

/** Builds a combo from a key press, or null for a bare modifier press. */
export function comboFromEvent(e: KeyboardEvent): string | null {
  if (MODIFIERS.has(e.key)) return null
  const parts: string[] = []
  if (e.ctrlKey || e.metaKey) parts.push('mod')
  if (e.altKey) parts.push('alt')
  if (e.shiftKey) parts.push('shift')
  parts.push(eventKey(e))
  return parts.join('+')
}

/** A combo needs Ctrl or Alt, so it does not clash with typing. */
export function hasModifier(combo: string): boolean {
  const parts = combo.split('+')
  return parts.includes('mod') || parts.includes('alt')
}

const KEY_LABELS: Record<string, string> = {
  mod: 'Ctrl',
  alt: 'Alt',
  shift: 'Shift',
  enter: 'Enter',
  space: 'Space',
  escape: 'Esc',
  arrowup: '↑',
  arrowdown: '↓',
  arrowleft: '←',
  arrowright: '→',
}

/** `mod+shift+s` → `Ctrl Shift S`. */
export function formatCombo(combo: string): string {
  return combo
    .split('+')
    .map((p) => KEY_LABELS[p] ?? (p.length === 1 ? p.toUpperCase() : p.charAt(0).toUpperCase() + p.slice(1)))
    .join(' ')
}
