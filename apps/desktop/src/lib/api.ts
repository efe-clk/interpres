// The only bridge between the UI and the Rust backend.
// Features without a backend yet return mock data so the UI stays navigable.

import { invoke } from '@tauri-apps/api/core'

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

export interface Settings {
  engine: EngineId
  fallback_engine: EngineId | null
  source_lang: string | null
  target_lang: string | null
  ui: { theme: 'dark' | 'light'; language: 'tr' | 'en' }
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

export interface GlossaryEntry {
  id: string
  source: string
  target: string
  sourceLang: string
  targetLang: string
}

export interface Glossary {
  id: string
  name: string
  sourceLang: string
  targetLang: string
}

export interface HistoryEntry {
  id: string
  sourceText: string
  targetText: string
  source: string | null
  target: string
  engine: EngineId
  createdAt: string
}

export type ClaudeCodeStatus = 'found' | 'not_found'

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
const uid = () => Math.random().toString(36).slice(2, 10)

// ---------- Browser-only mock state ----------

const defaultSettings: Settings = {
  engine: 'deepl',
  fallback_engine: null,
  source_lang: null,
  target_lang: 'EN-US',
  ui: { theme: 'dark', language: 'tr' },
}

function loadMockSettings(): Settings {
  try {
    const raw = localStorage.getItem('interpres.settings')
    if (raw) return { ...defaultSettings, ...JSON.parse(raw) }
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
  opts: { engine: EngineId; request: DocumentRequest; glossary: string | null },
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
    add(paths: string[], opts: { engine: EngineId; request: DocumentRequest; glossary: string | null }) {
      jobs = [...jobs, ...paths.map((p) => ({ ...job(p.split(/[\\/]/).pop() ?? p, 'queued', 0, opts.request.target), ...opts }))]
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

// ---------- Glossary (mock until M4) ----------

let glossaries: Glossary[] = [
  { id: 'g1', name: 'Genel', sourceLang: 'TR', targetLang: 'EN-US' },
  { id: 'g2', name: 'Teknik', sourceLang: 'TR', targetLang: 'DE' },
]
let terms: GlossaryEntry[] = [
  { id: uid(), source: 'çeviri belleği', target: 'translation memory', sourceLang: 'TR', targetLang: 'EN-US' },
  { id: uid(), source: 'terim sözlüğü', target: 'glossary', sourceLang: 'TR', targetLang: 'EN-US' },
  { id: uid(), source: 'kaynak dil', target: 'source language', sourceLang: 'TR', targetLang: 'EN-US' },
  { id: uid(), source: 'sunucu', target: 'Server', sourceLang: 'TR', targetLang: 'DE' },
]

export async function listGlossaries(): Promise<Glossary[]> {
  return glossaries
}

export async function listTerms(sourceLang: string, targetLang: string): Promise<GlossaryEntry[]> {
  return terms.filter((t) => t.sourceLang === sourceLang && t.targetLang === targetLang)
}

export async function upsertTerm(entry: Omit<GlossaryEntry, 'id'> & { id?: string }): Promise<void> {
  if (entry.id) terms = terms.map((t) => (t.id === entry.id ? { ...t, ...entry, id: t.id } : t))
  else terms = [...terms, { ...entry, id: uid() }]
  if (!glossaries.some((g) => g.sourceLang === entry.sourceLang && g.targetLang === entry.targetLang)) {
    glossaries = [...glossaries, { id: uid(), name: `${entry.sourceLang}-${entry.targetLang}`, sourceLang: entry.sourceLang, targetLang: entry.targetLang }]
  }
}

export async function deleteTerm(id: string): Promise<void> {
  terms = terms.filter((t) => t.id !== id)
}

export async function importTermsCsv(csv: string, sourceLang: string, targetLang: string): Promise<number> {
  const rows = csv.split(/\r?\n/).map((l) => l.split(',')).filter((r) => r.length >= 2 && r[0].trim())
  for (const [s, t] of rows) await upsertTerm({ source: s.trim(), target: t.trim(), sourceLang, targetLang })
  return rows.length
}

export async function exportTermsCsv(sourceLang: string, targetLang: string): Promise<string> {
  return (await listTerms(sourceLang, targetLang)).map((t) => `${t.source},${t.target}`).join('\n')
}

// ---------- History (mock until M4) ----------

const history: HistoryEntry[] = [
  { id: uid(), sourceText: 'Toplantı yarın saat onda başlayacak.', targetText: 'The meeting will start tomorrow at ten.', source: 'TR', target: 'EN-US', engine: 'deepl', createdAt: new Date(Date.now() - 36e5).toISOString() },
  { id: uid(), sourceText: 'Lütfen ekteki belgeyi inceleyin.', targetText: 'Bitte prüfen Sie das beigefügte Dokument.', source: 'TR', target: 'DE', engine: 'deepl', createdAt: new Date(Date.now() - 864e5).toISOString() },
  { id: uid(), sourceText: 'Der Vertrag endet am 31. Dezember.', targetText: 'Sözleşme 31 Aralık’ta sona eriyor.', source: 'DE', target: 'TR', engine: 'deepl', createdAt: new Date(Date.now() - 3 * 864e5).toISOString() },
]

export async function listHistory(query: string): Promise<HistoryEntry[]> {
  const q = query.trim().toLocaleLowerCase()
  if (!q) return history
  return history.filter((h) => `${h.sourceText} ${h.targetText}`.toLocaleLowerCase().includes(q))
}

// ---------- Claude and cache ----------

export async function claudeCodeStatus(): Promise<ClaudeCodeStatus> {
  if (isTauri) return (await call<boolean>('claude_code_found')) ? 'found' : 'not_found'
  return 'not_found'
}

export async function clearCache(): Promise<void> {
  await sleep(300)
}
