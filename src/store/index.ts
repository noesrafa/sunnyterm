import { create } from 'zustand'
import { subscribeWithSelector } from 'zustand/middleware'
import { toast } from 'sonner'
import type { Tile, TileKind, CanvasAction, DragState, Section, WorkspaceLayout, PersistedAppState, ViewMode } from '../types'
const GRID_SNAP = 12 // half of GRID_SPACING (24)
const TILE_MIN_W = 300
const TILE_MIN_H = 180

/** Snap a value to the nearest grid point */
function snapToGrid(v: number): number {
  return Math.round(v / GRID_SNAP) * GRID_SNAP
}
const UNDO_LIMIT = 50

/** The auto-save slot — not shown in the workspace list UI */
export const DEFAULT_WORKSPACE = '__default__'

let _sectionCounter = 0
function nextSectionId() { return `section-${Date.now()}-${_sectionCounter++}` }

let _tileCounter = 0
function nextId() {
  return `tile-${Date.now()}-${_tileCounter++}`
}
function nextZIndex(tiles: Tile[]) {
  return tiles.length === 0 ? 1 : Math.max(...tiles.map((t) => t.zIndex)) + 1
}

/** Check if a rect overlaps any existing tile */
function overlaps(tiles: Tile[], x: number, y: number, w: number, h: number): boolean {
  const GAP = 24
  return tiles.some((t) =>
    x < t.x + t.w + GAP &&
    x + w + GAP > t.x &&
    y < t.y + t.h + GAP &&
    y + h + GAP > t.y
  )
}

/** Find a non-overlapping position starting from (startX, startY).
 *  Tries the original position first, then scans right, then wraps to next row. */
function findFreePosition(tiles: Tile[], startX: number, startY: number, w: number, h: number): { x: number; y: number } {
  if (!overlaps(tiles, startX, startY, w, h)) return { x: startX, y: startY }

  const STEP_X = snapToGrid(w + 36)
  const STEP_Y = snapToGrid(h + 36)
  // Try positions to the right, then next row
  for (let row = 0; row < 20; row++) {
    for (let col = 0; col < 20; col++) {
      const cx = snapToGrid(startX + col * STEP_X)
      const cy = snapToGrid(startY + row * STEP_Y)
      if (!overlaps(tiles, cx, cy, w, h)) return { x: cx, y: cy }
    }
  }
  // Fallback: offset from last tile
  return { x: startX + tiles.length * 30, y: startY + tiles.length * 30 }
}

// ─── Store shape ──────────────────────────────────────────────────────────────

export interface CanvasStore {
  // Viewport
  zoom: number
  panX: number
  panY: number

  // Tiles
  tiles: Tile[]
  focusedId: string | null
  selectedIds: string[]

  // Sections
  sections: Section[]

  // Drag
  drag: DragState | null

  // Minimap
  showMinimap: boolean

  // Search
  searchOpen: boolean
  searchQuery: string

  // Linking
  linkingFromId: string | null

  // Undo
  undoStack: CanvasAction[]
  redoStack: CanvasAction[]

  // Dark mode
  isDark: boolean

  // View mode
  viewMode: ViewMode

  // UI state
  showShortcuts: boolean
  showConfirmClear: boolean
  savedToast: boolean
  exitedTileIds: string[]

  // ── Workspace ──────────────────────────────────────────────────────────────
  /** Named workspaces available (excludes __default__) */
  workspaces: string[]
  /** Currently active workspace name, null if unsaved session */
  activeWorkspace: string | null
  /** CWDs to restore when terminal tiles mount (populated on workspace load) */
  tileCwds: Record<string, string>
  /** Pending curl data to populate a newly spawned HTTP tile */
  pendingCurlData: Record<string, { method: string; url: string; headers: { key: string; value: string }[]; body: string }>

  // ── Actions ────────────────────────────────────────────────────────────────
  setZoom: (zoom: number) => void
  setPan: (x: number, y: number) => void
  zoomAt: (screenX: number, screenY: number, delta: number) => void
  panBy: (dx: number, dy: number) => void
  zoomIn: () => void
  zoomOut: () => void
  resetView: () => void
  fitAllTiles: () => void

  spawnTile: (kind: TileKind, x?: number, y?: number) => Tile
  removeTile: (id: string) => void
  focusTile: (id: string | null) => void
  renameTile: (id: string, name: string) => void
  autoRenameTile: (id: string, name: string) => void

  setSelectedIds: (ids: string[]) => void
  clearSelection: () => void

  createSection: (tileIds: string[]) => void
  removeSection: (id: string) => void
  renameSection: (id: string, name: string) => void
  duplicateSection: (id: string) => void
  moveSection: (id: string, dx: number, dy: number) => void
  resizeSection: (id: string, w: number, h: number) => void

  startDrag: (drag: DragState) => void
  updateDrag: (mouseX: number, mouseY: number) => void
  endDrag: () => void

  toggleMinimap: () => void
  toggleSearch: () => void
  setSearchQuery: (q: string) => void

  startLinking: (fromId: string) => void
  completeLinking: (toId: string) => void
  cancelLinking: () => void
  unlinkTile: (id: string) => void

  undo: () => void
  redo: () => void

  toggleDark: () => void
  setViewMode: (mode: ViewMode) => void
  toggleShortcuts: () => void
  toggleConfirmClear: () => void
  clearCanvas: () => void
  triggerSavedToast: () => void

  markTileExited: (id: string) => void
  markTileAlive: (id: string) => void

  // Workspace actions
  /** Save current layout. If name omitted, saves to active workspace or __default__. */
  saveWorkspace: (name?: string, explicit?: boolean) => Promise<void>
  /** Load a saved workspace by name and restore the layout. */
  loadWorkspace: (name: string) => Promise<void>
  /** Delete a saved workspace by name. */
  deleteWorkspace: (name: string) => Promise<void>
  /** Refresh the workspace list from disk. */
  refreshWorkspaces: () => Promise<void>
  /** Consume and clear the saved CWD for a tile (called by TerminalTile on mount). */
  consumeTileCwd: (tileId: string) => string | null
  /** Consume and clear pending curl data for an HTTP tile. */
  consumeCurlData: (tileId: string) => { method: string; url: string; headers: { key: string; value: string }[]; body: string } | null
  /** Init store from persisted state (called once on app start). */
  initFromPersisted: () => Promise<void>
}

// ─── Store ────────────────────────────────────────────────────────────────────

export const useStore = create<CanvasStore>()(
  subscribeWithSelector((set, get) => ({
    zoom: 1,
    panX: 0,
    panY: 0,
    tiles: [],
    focusedId: null,
    selectedIds: [],
    sections: [],
    drag: null,
    showMinimap: true,
    searchOpen: false,
    searchQuery: '',
    linkingFromId: null,
    undoStack: [],
    redoStack: [],
    isDark: true,
    viewMode: 'canvas' as ViewMode,
    showShortcuts: false,
    showConfirmClear: false,
    savedToast: false,
    exitedTileIds: [],
    workspaces: [],
    activeWorkspace: null,
    tileCwds: {},
    pendingCurlData: {},

    // ── Viewport ─────────────────────────────────────────────────────────────

    setZoom: (zoom) => set({ zoom: Math.max(0.5, Math.min(2, zoom)) }),

    setPan: (panX, panY) => set({ panX, panY }),

    panBy: (dx, dy) =>
      set((s) => ({ panX: s.panX + dx, panY: s.panY + dy })),

    zoomAt: (screenX, screenY, delta) => {
      const { zoom, panX, panY } = get()
      const factor = delta > 0 ? 1.1 : 0.9
      const newZoom = Math.max(0.5, Math.min(2, zoom * factor))
      const newPanX = screenX - (screenX - panX) * (newZoom / zoom)
      const newPanY = screenY - (screenY - panY) * (newZoom / zoom)
      set({ zoom: newZoom, panX: newPanX, panY: newPanY })
    },

    zoomIn: () => {
      const { zoom, panX, panY } = get()
      const cx = window.innerWidth / 2
      const cy = (window.innerHeight - 40) / 2
      const newZoom = Math.min(2, zoom * 1.2)
      set({
        zoom: newZoom,
        panX: cx - (cx - panX) * (newZoom / zoom),
        panY: cy - (cy - panY) * (newZoom / zoom)
      })
    },

    zoomOut: () => {
      const { zoom, panX, panY } = get()
      const cx = window.innerWidth / 2
      const cy = (window.innerHeight - 40) / 2
      const newZoom = Math.max(0.5, zoom / 1.2)
      set({
        zoom: newZoom,
        panX: cx - (cx - panX) * (newZoom / zoom),
        panY: cy - (cy - panY) * (newZoom / zoom)
      })
    },

    resetView: () => set({ zoom: 1, panX: 0, panY: 0 }),

    fitAllTiles: () => {
      const { tiles } = get()
      if (tiles.length === 0) { get().resetView(); return }
      const padding = 60
      const minX = Math.min(...tiles.map((t) => t.x)) - padding
      const minY = Math.min(...tiles.map((t) => t.y)) - padding
      const maxX = Math.max(...tiles.map((t) => t.x + t.w)) + padding
      const maxY = Math.max(...tiles.map((t) => t.y + t.h)) + padding
      const boxW = maxX - minX
      const boxH = maxY - minY
      const vpW = window.innerWidth
      const vpH = window.innerHeight - 40
      const newZoom = Math.max(0.5, Math.min(vpW / boxW, vpH / boxH, 2))
      const panX = (vpW - boxW * newZoom) / 2 - minX * newZoom
      const panY = (vpH - boxH * newZoom) / 2 - minY * newZoom
      set({ zoom: newZoom, panX, panY })
    },

    // ── Tiles ─────────────────────────────────────────────────────────────────

    spawnTile: (kind, x, y) => {
      const { tiles, panX, panY, zoom, viewMode } = get()
      const tileW = snapToGrid(640)
      const tileH = snapToGrid(396)

      let cx: number, cy: number
      if (x != null || y != null) {
        // Explicit position (e.g. double-click on canvas)
        cx = snapToGrid(x ?? 0)
        cy = snapToGrid(y ?? 0)
      } else if (viewMode === 'focus' || tiles.length > 0) {
        // In focus mode or when tiles exist: place near existing tiles in a grid
        // Find the bounding box of existing tiles and place below/right
        if (tiles.length === 0) {
          cx = snapToGrid(60)
          cy = snapToGrid(60)
        } else {
          const minX = Math.min(...tiles.map((t) => t.x))
          const maxY = Math.max(...tiles.map((t) => t.y + t.h))
          cx = snapToGrid(minX)
          cy = snapToGrid(maxY + 36)
        }
      } else {
        // Canvas mode, no tiles: center of viewport
        cx = snapToGrid((window.innerWidth / 2 - panX) / zoom - tileW / 2)
        cy = snapToGrid((window.innerHeight / 2 - panY) / zoom - tileH / 2)
      }
      const { x: safeX, y: safeY } = findFreePosition(tiles, cx, cy, tileW, tileH)
      const tile: Tile = {
        id: nextId(),
        x: safeX,
        y: safeY,
        w: tileW,
        h: tileH,
        name: (() => {
          const base = kind === 'terminal' ? 'Terminal' : kind === 'http' ? 'HTTP' : kind === 'postgres' ? 'PostgreSQL' : 'Browser'
          const count = tiles.filter((t) => t.kind === kind).length + 1
          return `${base} ${count}`
        })(),
        kind,
        userRenamed: false,
        outputLink: null,
        zIndex: nextZIndex(tiles)
      }
      pushUndo(get, set, { type: 'create', snapshot: { ...tile } })
      set((s) => ({ tiles: [...s.tiles, tile], focusedId: tile.id }))
      return tile
    },

    removeTile: (id) => {
      const tile = get().tiles.find((t) => t.id === id)
      if (!tile) return
      pushUndo(get, set, { type: 'delete', snapshot: { ...tile } })
      set((s) => ({
        tiles: s.tiles
          .filter((t) => t.id !== id)
          .map((t) => ({ ...t, outputLink: t.outputLink === id ? null : t.outputLink })),
        focusedId: s.focusedId === id ? null : s.focusedId,
        linkingFromId: s.linkingFromId === id ? null : s.linkingFromId,
        exitedTileIds: s.exitedTileIds.filter((eid) => eid !== id)
      }))
    },

    focusTile: (id) => {
      if (!id) { set({ focusedId: null }); return }
      set((s) => ({
        focusedId: id,
        tiles: s.tiles.map((t) =>
          t.id === id ? { ...t, zIndex: nextZIndex(s.tiles) } : t
        )
      }))
    },

    renameTile: (id, name) => {
      const tile = get().tiles.find((t) => t.id === id)
      if (!tile) return
      pushUndo(get, set, { type: 'rename', id, oldName: tile.name, newName: name })
      set((s) => ({
        tiles: s.tiles.map((t) => t.id === id ? { ...t, name, userRenamed: true } : t)
      }))
    },

    autoRenameTile: (id, name) => {
      if (!name) return
      set((s) => ({
        tiles: s.tiles.map((t) =>
          t.id === id && !t.userRenamed ? { ...t, name } : t
        )
      }))
    },

    // ── Selection ──────────────────────────────────────────────────────────

    setSelectedIds: (selectedIds) => set({ selectedIds }),
    clearSelection: () => set({ selectedIds: [] }),

    // ── Sections ──────────────────────────────────────────────────────────────

    createSection: (tileIds) => {
      const { tiles, sections } = get()
      const grouped = tiles.filter((t) => tileIds.includes(t.id))
      if (grouped.length === 0) return

      const PAD = 24
      const LABEL_H = 32
      const minX = Math.min(...grouped.map((t) => t.x)) - PAD
      const minY = Math.min(...grouped.map((t) => t.y)) - PAD - LABEL_H
      const maxX = Math.max(...grouped.map((t) => t.x + t.w)) + PAD
      const maxY = Math.max(...grouped.map((t) => t.y + t.h)) + PAD

      const section: Section = {
        id: nextSectionId(),
        name: `Section ${sections.length + 1}`,
        x: snapToGrid(minX),
        y: snapToGrid(minY),
        w: snapToGrid(maxX - minX),
        h: snapToGrid(maxY - minY)
      }
      set({ sections: [...sections, section], selectedIds: [] })
    },

    removeSection: (id) => {
      set((s) => ({ sections: s.sections.filter((sec) => sec.id !== id) }))
    },

    renameSection: (id, name) => {
      set((s) => ({
        sections: s.sections.map((sec) => sec.id === id ? { ...sec, name } : sec)
      }))
    },

    duplicateSection: (id) => {
      const { sections, tiles } = get()
      const sec = sections.find((s) => s.id === id)
      if (!sec) return

      // Offset: place to the right of the original with a gap
      const offsetX = sec.w + 60
      const offsetY = 0

      // Duplicate the section
      const dupSection: Section = {
        id: nextSectionId(),
        name: sec.name + ' (copy)',
        x: sec.x + offsetX,
        y: sec.y + offsetY,
        w: sec.w,
        h: sec.h
      }

      // Find contained tiles and duplicate them
      const contained = tiles.filter((t) => {
        const cx = t.x + t.w / 2
        const cy = t.y + t.h / 2
        return cx >= sec.x && cx <= sec.x + sec.w && cy >= sec.y && cy <= sec.y + sec.h
      })

      const newTiles = contained.map((t) => ({
        ...t,
        id: nextId(),
        x: t.x + offsetX,
        y: t.y + offsetY,
        outputLink: null,
        zIndex: nextZIndex([...tiles, ...contained]),
        userRenamed: false
      }))

      set({
        sections: [...sections, dupSection],
        tiles: [...tiles, ...newTiles]
      })
    },

    moveSection: (id, dx, dy) => {
      const { sections, tiles } = get()
      const sec = sections.find((s) => s.id === id)
      if (!sec) return
      // Find tiles inside this section (tile fully or mostly inside)
      const contained = tiles.filter((t) => {
        const cx = t.x + t.w / 2
        const cy = t.y + t.h / 2
        return cx >= sec.x && cx <= sec.x + sec.w && cy >= sec.y && cy <= sec.y + sec.h
      })
      set({
        sections: sections.map((s) =>
          s.id === id ? { ...s, x: snapToGrid(s.x + dx), y: snapToGrid(s.y + dy) } : s
        ),
        tiles: tiles.map((t) =>
          contained.some((c) => c.id === t.id)
            ? { ...t, x: snapToGrid(t.x + dx), y: snapToGrid(t.y + dy) }
            : t
        )
      })
    },

    resizeSection: (id, w, h) => {
      set((s) => ({
        sections: s.sections.map((sec) =>
          sec.id === id ? { ...sec, w: Math.max(120, snapToGrid(w)), h: Math.max(80, snapToGrid(h)) } : sec
        )
      }))
    },

    // ── Drag ─────────────────────────────────────────────────────────────────

    startDrag: (drag) => set({ drag }),

    updateDrag: (mouseX, mouseY) => {
      const { drag, tiles, zoom } = get()
      if (!drag) return

      const dx = (mouseX - drag.startMouseX) / zoom
      const dy = (mouseY - drag.startMouseY) / zoom

      set((s) => ({
        tiles: s.tiles.map((t) => {
          if (drag.kind === 'move') {
            // Group move: move all tiles that have a groupStart entry
            const gs = drag.groupStarts?.[t.id]
            if (gs) {
              return { ...t, x: snapToGrid(gs.x + dx), y: snapToGrid(gs.y + dy) }
            }
            if (t.id === drag.tileId) {
              return { ...t, x: snapToGrid(drag.startTileX + dx), y: snapToGrid(drag.startTileY + dy) }
            }
            return t
          } else {
            if (t.id !== drag.tileId) return t
            return {
              ...t,
              w: snapToGrid(Math.max(TILE_MIN_W, drag.startTileW + dx)),
              h: snapToGrid(Math.max(TILE_MIN_H, drag.startTileH + dy))
            }
          }
        })
      }))
    },

    endDrag: () => {
      const { drag, tiles } = get()
      if (!drag) return
      const tile = tiles.find((t) => t.id === drag.tileId)
      if (tile) {
        if (drag.kind === 'move') {
          if (tile.x !== drag.startTileX || tile.y !== drag.startTileY) {
            pushUndo(get, set, {
              type: 'move', id: tile.id,
              from: { x: drag.startTileX, y: drag.startTileY },
              to: { x: tile.x, y: tile.y }
            })
          }
        } else {
          if (tile.w !== drag.startTileW || tile.h !== drag.startTileH) {
            pushUndo(get, set, {
              type: 'resize', id: tile.id,
              from: { w: drag.startTileW, h: drag.startTileH },
              to: { w: tile.w, h: tile.h }
            })
          }
        }
      }
      set({ drag: null })
    },

    // ── UI ────────────────────────────────────────────────────────────────────

    toggleMinimap: () => set((s) => ({ showMinimap: !s.showMinimap })),
    toggleSearch: () => set((s) => ({ searchOpen: !s.searchOpen })),
    setSearchQuery: (searchQuery) => set({ searchQuery }),
    toggleShortcuts: () => set((s) => ({ showShortcuts: !s.showShortcuts })),
    toggleConfirmClear: () => set((s) => ({ showConfirmClear: !s.showConfirmClear })),
    clearCanvas: () => {
      const { tiles } = get()
      // Kill all PTYs
      for (const t of tiles) {
        if (t.kind === 'terminal') {
          window.electronAPI.ptyKill(t.id).catch(() => {})
        }
      }
      set({
        tiles: [],
        sections: [],
        focusedId: null,
        selectedIds: [],
        linkingFromId: null,
        undoStack: [],
        redoStack: [],
        exitedTileIds: [],
        showConfirmClear: false
      })
    },

    triggerSavedToast: () => {
      toast.success('Saved')
    },

    markTileExited: (id) =>
      set((s) => ({
        exitedTileIds: s.exitedTileIds.includes(id)
          ? s.exitedTileIds
          : [...s.exitedTileIds, id]
      })),

    markTileAlive: (id) =>
      set((s) => ({ exitedTileIds: s.exitedTileIds.filter((eid) => eid !== id) })),

    // ── Linking ───────────────────────────────────────────────────────────────

    startLinking: (fromId) => {
      set({ linkingFromId: fromId })
      toast('Click a tile to link output', { id: 'linking', duration: Infinity })
    },
    cancelLinking: () => {
      set({ linkingFromId: null })
      toast.dismiss('linking')
    },

    completeLinking: (toId) => {
      const { linkingFromId } = get()
      toast.dismiss('linking')
      if (!linkingFromId || linkingFromId === toId) {
        set({ linkingFromId: null })
        return
      }
      set((s) => ({
        tiles: s.tiles.map((t) =>
          t.id === linkingFromId ? { ...t, outputLink: toId } : t
        ),
        linkingFromId: null
      }))
      toast.success('Output linked')
    },

    unlinkTile: (id) => {
      set((s) => ({
        tiles: s.tiles.map((t) => t.id === id ? { ...t, outputLink: null } : t)
      }))
    },

    // ── Undo/redo ─────────────────────────────────────────────────────────────

    undo: () => {
      const { undoStack } = get()
      if (undoStack.length === 0) return
      const action = undoStack[undoStack.length - 1]
      set((s) => ({
        undoStack: s.undoStack.slice(0, -1),
        redoStack: [...s.redoStack, action]
      }))
      applyAction(get, set, action, true)
    },

    redo: () => {
      const { redoStack } = get()
      if (redoStack.length === 0) return
      const action = redoStack[redoStack.length - 1]
      set((s) => ({
        redoStack: s.redoStack.slice(0, -1),
        undoStack: [...s.undoStack, action]
      }))
      applyAction(get, set, action, false)
    },

    // ── Theme ─────────────────────────────────────────────────────────────────

    toggleDark: () => {
      const newDark = !get().isDark
      set({ isDark: newDark })
      window.electronAPI.appStateSave({
        isDark: newDark,
        lastWorkspace: get().activeWorkspace
      })
    },

    setViewMode: (viewMode) => {
      set({ viewMode })
      // When switching back to canvas, fit all tiles so nothing is off-screen
      if (viewMode === 'canvas') {
        requestAnimationFrame(() => get().fitAllTiles())
      }
      window.electronAPI.appStateSave({
        isDark: get().isDark,
        lastWorkspace: get().activeWorkspace,
        viewMode
      })
    },

    // ── Workspaces ────────────────────────────────────────────────────────────

    saveWorkspace: async (name, explicit = false) => {
      const { tiles, sections, zoom, panX, panY, activeWorkspace, isDark } = get()
      const workspaceName = name ?? activeWorkspace ?? DEFAULT_WORKSPACE

      // Collect CWDs for terminal tiles
      const tileCwds: Record<string, string> = {}
      for (const tile of tiles) {
        if (tile.kind === 'terminal') {
          const cwd = await window.electronAPI.ptyGetCwd(tile.id)
          if (cwd) tileCwds[tile.id] = cwd
        }
      }

      const layout: WorkspaceLayout = {
        name: workspaceName,
        tiles: tiles.map((t) => ({ ...t })),
        sections: sections.map((s) => ({ ...s })),
        canvasZoom: zoom,
        canvasPanX: panX,
        canvasPanY: panY,
        tileCwds
      }

      await window.electronAPI.workspaceSave(workspaceName, layout)

      // Persist app state (dark mode + last workspace) — merged in main process
      const appState: Partial<PersistedAppState> = {
        isDark,
        lastWorkspace: workspaceName !== DEFAULT_WORKSPACE ? workspaceName : null
      }
      await window.electronAPI.appStateSave(appState)

      const newName = workspaceName !== DEFAULT_WORKSPACE ? workspaceName : null
      set({ activeWorkspace: newName })

      const all = await window.electronAPI.workspaceList()
      set({ workspaces: all.filter((n) => n !== DEFAULT_WORKSPACE) })

      // Show save toast for explicit saves or named workspaces
      if (explicit || workspaceName !== DEFAULT_WORKSPACE) {
        get().triggerSavedToast()
      }
    },

    loadWorkspace: async (name) => {
      const layout = await window.electronAPI.workspaceLoad(name)
      if (!layout) return

      set({
        tiles: layout.tiles,
        sections: layout.sections ?? [],
        zoom: layout.canvasZoom,
        panX: layout.canvasPanX,
        panY: layout.canvasPanY,
        activeWorkspace: name !== DEFAULT_WORKSPACE ? name : null,
        focusedId: null,
        undoStack: [],
        redoStack: [],
        drag: null,
        tileCwds: layout.tileCwds ?? {},
        exitedTileIds: []
      })

      const { isDark } = get()
      const appState: Partial<PersistedAppState> = {
        isDark,
        lastWorkspace: name !== DEFAULT_WORKSPACE ? name : null
      }
      await window.electronAPI.appStateSave(appState)
    },

    deleteWorkspace: async (name) => {
      await window.electronAPI.workspaceDelete(name)
      const all = await window.electronAPI.workspaceList()
      set((s) => ({
        workspaces: all.filter((n) => n !== DEFAULT_WORKSPACE),
        activeWorkspace: s.activeWorkspace === name ? null : s.activeWorkspace
      }))
    },

    refreshWorkspaces: async () => {
      const all = await window.electronAPI.workspaceList()
      set({ workspaces: all.filter((n) => n !== DEFAULT_WORKSPACE) })
    },

    consumeTileCwd: (tileId) => {
      const { tileCwds } = get()
      const cwd = tileCwds[tileId] ?? null
      if (cwd) {
        set((s) => {
          const next = { ...s.tileCwds }
          delete next[tileId]
          return { tileCwds: next }
        })
      }
      return cwd
    },

    consumeCurlData: (tileId) => {
      const { pendingCurlData } = get()
      const data = pendingCurlData[tileId] ?? null
      if (data) {
        set((s) => {
          const next = { ...s.pendingCurlData }
          delete next[tileId]
          return { pendingCurlData: next }
        })
      }
      return data
    },

    initFromPersisted: async () => {
      const appState = await window.electronAPI.appStateLoad()
      if (appState) {
        set({ isDark: appState.isDark, viewMode: appState.viewMode ?? 'canvas' })
      }

      const all = await window.electronAPI.workspaceList()
      set({ workspaces: all.filter((n) => n !== DEFAULT_WORKSPACE) })

      const nameToLoad = appState?.lastWorkspace ?? DEFAULT_WORKSPACE
      const layout = await window.electronAPI.workspaceLoad(nameToLoad)

      if (layout && layout.tiles.length > 0) {
        set({
          tiles: layout.tiles,
          sections: layout.sections ?? [],
          zoom: layout.canvasZoom,
          panX: layout.canvasPanX,
          panY: layout.canvasPanY,
          activeWorkspace: nameToLoad !== DEFAULT_WORKSPACE ? nameToLoad : null,
          tileCwds: layout.tileCwds ?? {}
        })
      } else {
        const { spawnTile } = get()
        spawnTile('terminal', 60, 60)
      }
    }
  }))
)

// ─── Undo helpers ─────────────────────────────────────────────────────────────

function pushUndo(
  get: () => CanvasStore,
  set: (fn: (s: CanvasStore) => Partial<CanvasStore>) => void,
  action: CanvasAction
) {
  set((s) => ({
    undoStack: [...s.undoStack.slice(-UNDO_LIMIT + 1), action],
    redoStack: []
  }))
}

function applyAction(
  get: () => CanvasStore,
  set: (fn: (s: CanvasStore) => Partial<CanvasStore>) => void,
  action: CanvasAction,
  reverse: boolean
) {
  switch (action.type) {
    case 'move':
      set((s) => ({
        tiles: s.tiles.map((t) =>
          t.id === action.id
            ? { ...t, ...(reverse ? action.from : action.to) }
            : t
        )
      }))
      break
    case 'resize':
      set((s) => ({
        tiles: s.tiles.map((t) =>
          t.id === action.id
            ? { ...t, ...(reverse ? action.from : action.to) }
            : t
        )
      }))
      break
    case 'create':
      if (reverse) {
        set((s) => ({ tiles: s.tiles.filter((t) => t.id !== action.snapshot.id) }))
      } else {
        set((s) => ({ tiles: [...s.tiles, action.snapshot] }))
      }
      break
    case 'delete':
      if (reverse) {
        set((s) => ({ tiles: [...s.tiles, action.snapshot] }))
      } else {
        set((s) => ({ tiles: s.tiles.filter((t) => t.id !== action.snapshot.id) }))
      }
      break
    case 'rename':
      set((s) => ({
        tiles: s.tiles.map((t) =>
          t.id === action.id
            ? { ...t, name: reverse ? action.oldName : action.newName }
            : t
        )
      }))
      break
  }
}
