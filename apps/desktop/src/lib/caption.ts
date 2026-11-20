import type { CaptionStyle } from '@/lib/api'

/** CSS for a caption panel, matching the native overlay renderer. */
export function captionCss(style: CaptionStyle): { panel: React.CSSProperties; text: React.CSSProperties } {
  const hex = /^#?([0-9a-f]{6})$/i.exec(style.background)?.[1] ?? '101014'
  const [r, g, b] = [0, 2, 4].map((i) => parseInt(hex.slice(i, i + 2), 16))
  const luminance = (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255
  const dark = style.opacity < 30 || luminance < 0.55
  return {
    panel: { background: `rgba(${r}, ${g}, ${b}, ${style.opacity / 100})` },
    text: {
      color: dark ? '#ffffff' : '#141418',
      fontSize: style.font_size ? `${style.font_size}px` : undefined,
      textShadow: style.opacity < 30 ? '0 1px 3px rgba(0,0,0,0.9)' : undefined,
    },
  }
}
