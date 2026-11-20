// The only bridge between the UI and the Rust backend.
// Features without a backend yet return mock data so the UI stays navigable.

import { invoke } from '@tauri-apps/api/core'
import { DEFAULT_SHORTCUTS } from '@/lib/shortcuts'

export const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

// ---------- Types (mirror interpres-core) ----------

export type EngineId = 'deepl' | 'claude-api' | 'claude-code'
export type Formality = 'default' | 'more' | 'less' | 'prefer_more' | 'prefer_less'
export type LanguageKind = 'source' | 'target'

export interface TranslateRequest {
  text: string
  source: string | null
  target: string
  formality: Formality
}

export interface Translation {
  text: string
  detected_source: string | null
  engine: EngineId
}

export type ErrorKind =
  | 'missing_key'
  | 'invalid_key'
  | 'quota_exceeded'
  | 'rate_limited'
  | 'network'
  | 'invalid_request'
  | 'service'
  | 'cancelled'

export interface ApiError {
  kind: ErrorKind
  message?: string
}

export interface Shortcuts {
  palette: string
  translate: string
  swap: string
  copy: string
  add_files: string
  /** System-wide: starts or stops the game overlay. */
  overlay: string
  /** System-wide: opens the quick-translate window. */
  quick: string
  /** System-wide: shows the main window, or sends it to the tray. */
  toggle: string
}

export interface Settings {
  engine: EngineId
  fallback_engine: EngineId | null
  source_lang: string | null
  target_lang: string | null
  ui: { theme: 'dark' | 'light'; language: 'tr' | 'en'; shortcuts: Shortcuts }
  game: GameSettings
}

export type OcrModel = 'general' | 'korean' | 'latin'

export interface GameSettings {
  /** x, y, width, height in logical screen coordinates. */
  region: [number, number, number, number] | null
  ocr_model: OcrModel
  source_lang: string | null
  target_lang: string | null
}

export interface KeyStatus {
  deepl: 'free' | 'pro' | null
}

export interface Usage {
  character_count: number
  character_limit: number
}

export type JobStatus =
  | 'queued'
  | 'uploading'
  | 'translating'
  | 'downloading'
  | 'done'
  | 'error'
  | 'cancelled'

export interface DocumentRequest {
  source: string | null
  target: string
  formality: Formality
}

export interface DocumentJob {
  id: number
  name: string
  input: string
  output: string | null
  engine: EngineId
  request: DocumentRequest
  glossary: string | null
  status: JobStatus
  progress: number
  error: ApiError | null
}

/** A glossary term. Language codes are base codes (`TR`, `EN`). */
export interface Term {
  id: number
  source: string
  target: string
}

export interface GlossaryPair {
  source_lang: string
  target_lang: string
  count: number
}

export interface HistoryEntry {
  id: number
  source_text: string
  target_text: string
  source_lang: string | null
  target_lang: string
  detected_source: string | null
  engine: EngineId
  /** Unix seconds. */
  created_at: number
}

// ---------- Helpers ----------

function toApiError(e: unknown): ApiError {
  if (e && typeof e === 'object' && 'kind' in e) return e as ApiError
  return { kind: 'service', message: String(e) }
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args)
  } catch (e) {
    throw toApiError(e)
  }
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))

// ---------- Browser-only mock state ----------

const defaultSettings: Settings = {
  engine: 'deepl',
  fallback_engine: null,
  source_lang: null,
  target_lang: 'EN-US',
  ui: {
    theme: 'dark',
    language: 'tr',
    shortcuts: DEFAULT_SHORTCUTS,
  },
  game: { region: null, ocr_model: 'general', source_lang: null, target_lang: null },
}

function loadMockSettings(): Settings {
  try {
    const raw = localStorage.getItem('interpres.settings')
    if (raw) {
      const s = JSON.parse(raw) as Partial<Settings>
      return {
        ...defaultSettings,
        ...s,
        ui: { ...defaultSettings.ui, ...s.ui, shortcuts: { ...DEFAULT_SHORTCUTS, ...s.ui?.shortcuts } },
        game: { ...defaultSettings.game, ...s.game },
      }
    }
  } catch {
    // ignore
  }
  return defaultSettings
}

let mockKey: KeyStatus = { deepl: null }

// ---------- Text, keys, settings (real in Tauri) ----------

export async function translate(req: TranslateRequest): Promise<Translation> {
  if (isTauri) return call('translate', { req })
  await sleep(500)
  if (!navigator.onLine) throw { kind: 'network' } satisfies ApiError
  return { text: `[${req.target}] ${req.text}`, detected_source: req.source ?? 'TR', engine: 'deepl' }
}

export async function keyStatus(): Promise<KeyStatus> {
  if (isTauri) return call('key_status')
  return mockKey
}

export async function setDeeplKey(key: string): Promise<'free' | 'pro'> {
  if (isTauri) return call('set_deepl_key', { key })
  await sleep(400)
  const plan = key.trim().endsWith(':fx') ? 'free' : 'pro'
  mockKey = { deepl: plan }
  return plan
}

export async function deleteDeeplKey(): Promise<void> {
  if (isTauri) return call('delete_deepl_key')
  mockKey = { deepl: null }
}

export async function deeplUsage(): Promise<Usage> {
  if (isTauri) return call('deepl_usage')
  return { character_count: 12840, character_limit: 500000 }
}

export async function getSettings(): Promise<Settings> {
  if (isTauri) return call('get_settings')
  return loadMockSettings()
}

export async function saveSettings(settings: Settings): Promise<void> {
  if (isTauri) return call('save_settings', { settings })
  localStorage.setItem('interpres.settings', JSON.stringify(settings))
}

// ---------- Clipboard and windows ----------

export async function readClipboard(): Promise<string> {
  if (isTauri) {
    const { readText } = await import('@tauri-apps/plugin-clipboard-manager')
    // Rejects when the clipboard is empty or holds non-text data.
    return (await readText().catch(() => '')) ?? ''
  }
  return navigator.clipboard.readText().catch(() => '')
}

export async function writeClipboard(text: string): Promise<void> {
  if (isTauri) {
    const { writeText } = await import('@tauri-apps/plugin-clipboard-manager')
    return writeText(text)
  }
  return navigator.clipboard.writeText(text)
}

export async function hideCurrentWindow(): Promise<void> {
  if (!isTauri) return
  const { getCurrentWindow } = await import('@tauri-apps/api/window')
  await getCurrentWindow().hide()
}

/** Called when the quick-translate window is summoned. Returns an unsubscribe fn. */
export async function onQuickOpen(cb: () => void): Promise<() => void> {
  if (!isTauri) return () => {}
  const { listen } = await import('@tauri-apps/api/event')
  return listen('quick-open', cb)
}

// ---------- Documents ----------

export const DOCUMENT_EXTENSIONS = ['docx', 'pptx', 'xlsx', 'pdf', 'srt', 'md', 'txt']

/**
 * In Tauri, file drops are delivered by the webview, not as HTML5 drop events.
 * Calls `onHover(true/false)` while dragging and `onDrop(paths)` on drop.
 */
export async function onFileDrop(
  onHover: (over: boolean) => void,
  onDrop: (paths: string[]) => void,
): Promise<() => void> {
  if (!isTauri) return () => {}
  const { getCurrentWebview } = await import('@tauri-apps/api/webview')
  return getCurrentWebview().onDragDropEvent((e) => {
    if (e.payload.type === 'enter' || e.payload.type === 'over') onHover(true)
    else if (e.payload.type === 'leave') onHover(false)
    else if (e.payload.type === 'drop') {
      onHover(false)
      onDrop(e.payload.paths)
    }
  })
}

/** Opens the system file picker. Returns the chosen paths (empty if cancelled). */
export async function pickDocuments(): Promise<string[]> {
  if (!isTauri) return ['ornek-sozlesme.docx']
  const { open } = await import('@tauri-apps/plugin-dialog')
  const picked = await open({
    multiple: true,
    filters: [{ name: 'Documents', extensions: DOCUMENT_EXTENSIONS }],
  })
  return picked ?? []
}

/** Calls `cb` with the job list now and on every change. Returns an unsubscribe fn. */
export function subscribeJobs(cb: (jobs: DocumentJob[]) => void): () => void {
  if (!isTauri) return mockDocs.subscribe(cb)
  let off: (() => void) | undefined
  let stopped = false
  void (async () => {
    const { listen } = await import('@tauri-apps/api/event')
    const unlisten = await listen<DocumentJob[]>('documents-changed', (e) => cb(e.payload))
    if (stopped) unlisten()
    else off = unlisten
    cb(await call<DocumentJob[]>('documents_list'))
  })()
  return () => {
    stopped = true
    off?.()
  }
}

export async function addDocuments(
  paths: string[],
  opts: { engine: EngineId; request: DocumentRequest; useGlossary: boolean },
): Promise<void> {
  if (isTauri) {
    await call('documents_add', { paths, ...opts })
    return
  }
  mockDocs.add(paths, opts)
}

export async function cancelDocument(id: number): Promise<void> {
  if (isTauri) return call('documents_cancel', { id })
  mockDocs.update(id, (j) => ({ ...j, status: 'cancelled' }))
}

export async function removeDocument(id: number): Promise<void> {
  if (isTauri) return call('documents_remove', { id })
  mockDocs.remove(id)
}

export async function revealDocument(id: number): Promise<void> {
  if (isTauri) return call('documents_reveal', { id })
}

export async function openDocument(id: number): Promise<void> {
  if (isTauri) return call('documents_open', { id })
}

/** Browser-only stand-in for the Rust queue. */
const mockDocs = (() => {
  let nextId = 1
  const job = (name: string, status: JobStatus, progress: number, target: string, error: ApiError | null = null): DocumentJob => ({
    id: nextId++,
    name,
    input: `/home/user/${name}`,
    output: status === 'done' ? `/home/user/${name}` : null,
    engine: 'deepl',
    request: { source: null, target, formality: 'default' },
    glossary: null,
    status,
    progress,
    error,
  })
  let jobs: DocumentJob[] = [
    job('rapor-2026.docx', 'done', 100, 'EN-US'),
    job('sunum.pptx', 'translating', 45, 'DE'),
    job('butce.xlsx', 'error', 0, 'EN-US', { kind: 'quota_exceeded' }),
  ]
  const listeners = new Set<(j: DocumentJob[]) => void>()
  const emit = () => listeners.forEach((l) => l([...jobs]))
  setInterval(() => {
    let changed = false
    jobs = jobs.map((j) => {
      if (['done', 'error', 'cancelled'].includes(j.status)) return j
      changed = true
      const progress = Math.min(100, j.progress + 7)
      const status: JobStatus =
        progress >= 100 ? 'done' : progress < 10 ? 'uploading' : progress > 90 ? 'downloading' : 'translating'
      return { ...j, progress, status }
    })
    if (changed) emit()
  }, 700)
  return {
    subscribe(cb: (j: DocumentJob[]) => void) {
      listeners.add(cb)
      cb([...jobs])
      return () => void listeners.delete(cb)
    },
    add(paths: string[], opts: { engine: EngineId; request: DocumentRequest; useGlossary: boolean }) {
      jobs = [
        ...jobs,
        ...paths.map((p) => ({
          ...job(p.split(/[\\/]/).pop() ?? p, 'queued', 0, opts.request.target),
          engine: opts.engine,
          request: opts.request,
        })),
      ]
      emit()
    },
    update(id: number, f: (j: DocumentJob) => DocumentJob) {
      jobs = jobs.map((j) => (j.id === id ? f(j) : j))
      emit()
    },
    remove(id: number) {
      jobs = jobs.filter((j) => j.id !== id)
      emit()
    },
  }
})()

// ---------- Glossary ----------

/** Browser-only stand-in for the SQLite glossary. */
const mockTerms: (Term & { source_lang: string; target_lang: string })[] = [
  { id: 1, source: 'çeviri belleği', target: 'translation memory', source_lang: 'TR', target_lang: 'EN' },
  { id: 2, source: 'terim sözlüğü', target: 'glossary', source_lang: 'TR', target_lang: 'EN' },
  { id: 3, source: 'sunucu', target: 'Server', source_lang: 'TR', target_lang: 'DE' },
]

export async function listGlossaryPairs(): Promise<GlossaryPair[]> {
  if (isTauri) return call('glossary_pairs')
  const pairs = new Map<string, GlossaryPair>()
  for (const t of mockTerms) {
    const k = `${t.source_lang}-${t.target_lang}`
    const p = pairs.get(k) ?? { source_lang: t.source_lang, target_lang: t.target_lang, count: 0 }
    p.count++
    pairs.set(k, p)
  }
  return [...pairs.values()]
}

export async function listTerms(sourceLang: string, targetLang: string): Promise<Term[]> {
  if (isTauri) return call('glossary_terms', { sourceLang, targetLang })
  return mockTerms.filter((t) => t.source_lang === sourceLang && t.target_lang === targetLang)
}

export async function upsertTerm(t: {
  id?: number
  sourceLang: string
  targetLang: string
  source: string
  target: string
}): Promise<void> {
  if (isTauri) return call('glossary_upsert', { ...t, id: t.id ?? null })
  const i = mockTerms.findIndex((x) => x.id === t.id)
  const row = { id: t.id ?? Date.now(), source: t.source, target: t.target, source_lang: t.sourceLang, target_lang: t.targetLang }
  if (i >= 0) mockTerms[i] = row
  else mockTerms.push(row)
}

export async function deleteTerm(id: number): Promise<void> {
  if (isTauri) return call('glossary_delete', { id })
  const i = mockTerms.findIndex((x) => x.id === id)
  if (i >= 0) mockTerms.splice(i, 1)
}

/** Asks for a CSV file and imports it. Returns the number of terms, or null if cancelled. */
export async function importTermsCsv(sourceLang: string, targetLang: string): Promise<number | null> {
  if (!isTauri) return 0
  const { open } = await import('@tauri-apps/plugin-dialog')
  const path = await open({ filters: [{ name: 'CSV', extensions: ['csv'] }] })
  if (!path) return null
  return call('glossary_import', { path, sourceLang, targetLang })
}

/** Asks where to save and exports the terms as CSV. Returns the count, or null if cancelled. */
export async function exportTermsCsv(sourceLang: string, targetLang: string): Promise<number | null> {
  if (!isTauri) return 0
  const { save } = await import('@tauri-apps/plugin-dialog')
  const path = await save({
    defaultPath: `glossary-${sourceLang}-${targetLang}.csv`,
    filters: [{ name: 'CSV', extensions: ['csv'] }],
  })
  if (!path) return null
  return call('glossary_export', { path, sourceLang, targetLang })
}

// ---------- History ----------

const mockHistory: HistoryEntry[] = [
  { id: 1, source_text: 'Toplantı yarın saat onda başlayacak.', target_text: 'The meeting will start tomorrow at ten.', source_lang: 'TR', target_lang: 'EN-US', detected_source: 'TR', engine: 'deepl', created_at: Date.now() / 1000 - 3600 },
  { id: 2, source_text: 'Der Vertrag endet am 31. Dezember.', target_text: 'Sözleşme 31 Aralık’ta sona eriyor.', source_lang: null, target_lang: 'TR', detected_source: 'DE', engine: 'deepl', created_at: Date.now() / 1000 - 86400 },
]

export async function listHistory(query: string): Promise<HistoryEntry[]> {
  if (isTauri) return call('history_list', { query })
  const q = query.trim().toLocaleLowerCase()
  return mockHistory.filter((h) => !q || `${h.source_text} ${h.target_text}`.toLocaleLowerCase().includes(q))
}

// ---------- Game overlay ----------

export interface OcrStatus {
  installed: boolean
  /** Download size in bytes. */
  size: number
}

export interface GameFrame {
  source: string
  translation: string
}

export interface GameState {
  running: boolean
  error: ApiError | null
}

async function listenTo<T>(event: string, cb: (payload: T) => void): Promise<() => void> {
  if (!isTauri) return () => {}
  const { listen } = await import('@tauri-apps/api/event')
  return listen<T>(event, (e) => cb(e.payload))
}

let mockOcrInstalled = false
let mockRunning = false
const mockGameListeners = new Set<(s: GameState) => void>()

export async function ocrStatus(model: OcrModel): Promise<OcrStatus> {
  if (isTauri) return call('ocr_status', { model })
  return { installed: mockOcrInstalled, size: 21_400_000 }
}

export async function ocrDownload(model: OcrModel, onProgress: (done: number, total: number) => void): Promise<void> {
  if (!isTauri) {
    for (let i = 1; i <= 10; i++) {
      await sleep(120)
      onProgress(i * 2_140_000, 21_400_000)
    }
    mockOcrInstalled = true
    return
  }
  const off = await listenTo<{ done: number; total: number }>('ocr-download', (p) => onProgress(p.done, p.total))
  try {
    await call('ocr_download', { model })
  } finally {
    off()
  }
}

/** Lets the user drag a rectangle on screen. Null when cancelled. */
export async function gamePickRegion(): Promise<[number, number, number, number] | null> {
  if (isTauri) return call('game_pick_region')
  await sleep(300)
  return [320, 760, 960, 140]
}

/** Used by the window-based picker page. */
export async function gameRegionPicked(region: [number, number, number, number] | null): Promise<void> {
  if (isTauri) return call('game_region_picked', { region })
}

export async function gameStatus(): Promise<boolean> {
  if (isTauri) return call('game_status')
  return mockRunning
}

export async function gameStart(): Promise<void> {
  if (isTauri) return call('game_start')
  mockRunning = true
  mockGameListeners.forEach((l) => l({ running: true, error: null }))
}

export async function gameStop(): Promise<void> {
  if (isTauri) return call('game_stop')
  mockRunning = false
  mockGameListeners.forEach((l) => l({ running: false, error: null }))
}

export async function onGameFrame(cb: (f: GameFrame) => void): Promise<() => void> {
  if (!isTauri) {
    const t = setInterval(() => mockRunning && cb({ source: '扉は鍵がかかっている。', translation: 'Kapı kilitli.' }), 1500)
    return () => clearInterval(t)
  }
  return listenTo('game-frame', cb)
}

export async function onGameState(cb: (s: GameState) => void): Promise<() => void> {
  if (!isTauri) {
    mockGameListeners.add(cb)
    return () => void mockGameListeners.delete(cb)
  }
  return listenTo('game-state', cb)
}

// ---------- Cache ----------

export async function clearCache(): Promise<void> {
  if (isTauri) return call('cache_clear')
  await sleep(300)
}
