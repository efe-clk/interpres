// Pages for the window-based overlay (Windows, X11): the region picker and the
// caption. On Wayland compositors with layer-shell these are drawn natively.

import { useEffect, useRef, useState } from 'react'
import * as api from '@/lib/api'

/** Full-screen transparent picker: drag a rectangle, Esc cancels. */
export function PickerPage() {
  const [start, setStart] = useState<[number, number] | null>(null)
  const [end, setEnd] = useState<[number, number] | null>(null)
  const done = useRef(false)

  const finish = (region: [number, number, number, number] | null) => {
    if (done.current) return
    done.current = true
    void api.gameRegionPicked(region)
  }

  useEffect(() => {
    document.documentElement.style.background = 'transparent'
    document.body.style.background = 'transparent'
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && finish(null)
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  const rect = start && end
    ? {
        x: Math.min(start[0], end[0]),
        y: Math.min(start[1], end[1]),
        w: Math.abs(end[0] - start[0]),
        h: Math.abs(end[1] - start[1]),
      }
    : null

  return (
    <div
      className="fixed inset-0 cursor-crosshair select-none"
      style={{ background: 'rgba(0,0,0,0.35)' }}
      onMouseDown={(e) => {
        if (e.button !== 0) return finish(null)
        setStart([e.clientX, e.clientY])
        setEnd([e.clientX, e.clientY])
      }}
      onMouseMove={(e) => start && setEnd([e.clientX, e.clientY])}
      onMouseUp={() => {
        if (rect && rect.w >= 16 && rect.h >= 8) finish([Math.round(rect.x), Math.round(rect.y), Math.round(rect.w), Math.round(rect.h)])
        else {
          setStart(null)
          setEnd(null)
        }
      }}
    >
      {rect && (
        <div
          className="absolute border-2 border-primary"
          style={{ left: rect.x, top: rect.y, width: rect.w, height: rect.h, boxShadow: '0 0 0 9999px rgba(0,0,0,0.2)' }}
        />
      )}
    </div>
  )
}

/** Click-through caption window placed next to the captured region. */
export function CaptionPage() {
  const [text, setText] = useState('')

  useEffect(() => {
    document.documentElement.style.background = 'transparent'
    document.body.style.background = 'transparent'
    let off = () => {}
    void api.onGameFrame((f) => setText(f.translation)).then((f) => (off = f))
    return () => off()
  }, [])

  if (!text) return null
  return (
    <div className="fixed inset-0 flex items-center justify-center rounded-xl px-4 text-center" style={{ background: 'rgba(16,16,20,0.85)' }}>
      <p className="text-[clamp(14px,4.5vh,30px)] leading-snug font-medium text-white">{text}</p>
    </div>
  )
}
