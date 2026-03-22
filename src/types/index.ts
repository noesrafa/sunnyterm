// ─── Tile kinds ───────────────────────────────────────────────────────────────

export type TileKind = 'terminal' | 'http' | 'postgres' | 'browser'

// ─── Section (Figma-style grouping) ──────────────────────────────────────────

export interface Section {
  id: string
  name: string
  x: number
  y: number
  w: number
  h: number
}

// ─── Canvas tile ──────────────────────────────────────────────────────────────

export interface Tile {
  id: string
  x: number
  y: number
  w: number
  h: number
  name: string
  kind: TileKind
  userRenamed: boolean
  outputLink: string | null  // tile id this tile pipes output to
  zIndex: number
  initialUrl?: string  // for browser tiles: URL to load on first mount
}

// Tile snapshot for undo/redo
export type TileSnapshot = Tile

// ─── Undo/redo ────────────────────────────────────────────────────────────────

export type CanvasAction =
  | { type: 'move'; id: string; from: { x: number; y: number }; to: { x: number; y: number } }
  | { type: 'resize'; id: string; from: { w: number; h: number }; to: { w: number; h: number } }
  | { type: 'create'; snapshot: TileSnapshot }
  | { type: 'delete'; snapshot: TileSnapshot }
  | { type: 'rename'; id: string; oldName: string; newName: string }

// ─── Canvas drag state ────────────────────────────────────────────────────────

export type DragKind = 'move' | 'resize'

export interface DragState {
  tileId: string
  kind: DragKind
  startMouseX: number
  startMouseY: number
  startTileX: number
  startTileY: number
  startTileW: number
  startTileH: number
  /** Starting positions of all selected tiles (for group move) */
  groupStarts?: Record<string, { x: number; y: number }>
}

// ─── Snapping ─────────────────────────────────────────────────────────────────

export interface SnapResult {
  x: number
  y: number
}

// ─── Workspace ────────────────────────────────────────────────────────────────

export interface WorkspaceLayout {
  name: string
  tiles: TileSnapshot[]
  sections?: Section[]
  canvasZoom: number
  canvasPanX: number
  canvasPanY: number
  /** CWD per terminal tile id */
  tileCwds?: Record<string, string>
  savedAt?: string
}

// ─── App state (persisted) ────────────────────────────────────────────────────

export type ViewMode = 'canvas' | 'focus'

export interface PersistedAppState {
  isDark: boolean
  lastWorkspace: string | null
  viewMode?: ViewMode
  windowBounds?: { x: number; y: number; width: number; height: number }
}

// ─── ElectronAPI (window.electronAPI injected by preload) ─────────────────────

export interface ElectronAPI {
  // PTY
  ptySpawn: (id: string, shell: string, cols: number, rows: number, cwd?: string) => Promise<number>
  ptyHas: (id: string) => Promise<boolean>
  ptyReattach: (id: string) => Promise<boolean>
  ptyWrite: (id: string, data: string) => Promise<void>
  ptyResize: (id: string, cols: number, rows: number) => Promise<void>
  ptyKill: (id: string) => Promise<void>
  ptyGetCwd: (id: string) => Promise<string | null>
  onPtyData: (id: string, callback: (data: string) => void) => () => void
  onPtyExit: (id: string, callback: (code: number) => void) => () => void

  // Menu actions from main process
  onMenuAction: (callback: (action: string) => void) => () => void

  // URL open requests (link clicks in terminals/webviews)
  onOpenUrl: (callback: (url: string) => void) => () => void

  // Workspaces
  workspaceList: () => Promise<string[]>
  workspaceSave: (name: string, layout: WorkspaceLayout) => Promise<void>
  workspaceLoad: (name: string) => Promise<WorkspaceLayout | null>
  workspaceDelete: (name: string) => Promise<void>

  // App state
  appStateLoad: () => Promise<PersistedAppState | null>
  appStateSave: (state: Partial<PersistedAppState>) => Promise<void>

  // HTTP requests (proxied through main to avoid CORS)
  httpRequest: (opts: {
    method: string
    url: string
    headers: Record<string, string>
    body: string | null
  }) => Promise<HttpResponse>

  // PostgreSQL
  pgConnect: (id: string, connectionString: string) => Promise<{ ok: boolean; error?: string }>
  pgDisconnect: (id: string) => Promise<void>
  pgQuery: (id: string, sql: string) => Promise<PgQueryResult>
}

// ─── HTTP types ───────────────────────────────────────────────────────────────

export interface HttpResponse {
  ok: boolean
  status?: number
  statusText?: string
  headers?: Record<string, string>
  body?: string
  elapsed?: number
  error?: string
}

export interface HttpRequestEntry {
  method: string
  url: string
  headers: { key: string; value: string }[]
  body: string
  timestamp: number
  response?: HttpResponse
}

// ─── PostgreSQL types ─────────────────────────────────────────────────────────

export interface PgQueryResult {
  ok: boolean
  fields?: string[]
  rows?: Record<string, unknown>[]
  rowCount?: number | null
  elapsed?: number
  error?: string
}

declare global {
  interface Window {
    electronAPI: ElectronAPI
  }
}
