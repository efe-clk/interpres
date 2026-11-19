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

export type JobStatus = 'queued' | 'uploading' | 'translating' | 'downloading' | 'done' | 'error'

export interface DocumentJob {
  id: string
  name: string
  status: JobStatus
  progress: number
  engine: EngineId
  target: string
  glossary: string | null
  error?: ErrorKind
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

// ---------- Documents (mock until M3) ----------

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

let jobs: DocumentJob[] = [
  { id: uid(), name: 'rapor-2026.docx', status: 'done', progress: 100, engine: 'deepl', target: 'EN-US', glossary: null },
  { id: uid(), name: 'sunum.pptx', status: 'translating', progress: 45, engine: 'deepl', target: 'DE', glossary: 'Teknik' },
  { id: uid(), name: 'butce.xlsx', status: 'error', progress: 0, engine: 'deepl', target: 'EN-US', glossary: null, error: 'quota_exceeded' },
]
const jobListeners = new Set<(j: DocumentJob[]) => void>()
const emitJobs = () => jobListeners.forEach((l) => l([...jobs]))

setInterval(() => {
  let changed = false
  jobs = jobs.map((j) => {
    if (j.status === 'done' || j.status === 'error') return j
    changed = true
    const progress = Math.min(100, j.progress + 7)
    const status: JobStatus =
      progress >= 100 ? 'done' : progress < 10 ? 'uploading' : progress > 90 ? 'downloading' : 'translating'
    return { ...j, progress, status }
  })
  if (changed) emitJobs()
}, 700)

export function subscribeJobs(cb: (j: DocumentJob[]) => void): () => void {
  jobListeners.add(cb)
  cb([...jobs])
  return () => jobListeners.delete(cb)
}

export async function addDocuments(
  names: string[],
  opts: { engine: EngineId; target: string; glossary: string | null },
): Promise<void> {
  jobs = [...jobs, ...names.map((name) => ({ id: uid(), name, status: 'queued' as const, progress: 0, ...opts }))]
  emitJobs()
}

export async function removeDocument(id: string): Promise<void> {
  jobs = jobs.filter((j) => j.id !== id)
  emitJobs()
}

export async function revealDocument(_id: string): Promise<void> {}
export async function openDocument(_id: string): Promise<void> {}

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
