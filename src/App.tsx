import React, { useEffect, useRef } from 'react'
import { InfiniteCanvas } from './canvas/InfiniteCanvas'
import { SearchBar } from './search/SearchBar'
import { WorkspacePicker } from './workspace/WorkspacePicker'
import { useKeyboard } from './hooks/useKeyboard'
import { useStore, DEFAULT_WORKSPACE } from './store'
import { Terminal, Globe, Database, Undo2, Redo2, Map, Search, Maximize, Sun, Moon, ZoomIn, ZoomOut, FolderOpen } from 'lucide-react'

// ── Auto-save debounce (ms) ───────────────────────────────────────────────────
const AUTO_SAVE_DEBOUNCE_MS = 2_000

// ─── App ──────────────────────────────────────────────────────────────────────

export function App() {
  return <AppInner />
}

function AppInner() {
  useKeyboard()
  const isDark = useStore((s) => s.isDark)
  const showShortcuts = useStore((s) => s.showShortcuts)
  const showConfirmClear = useStore((s) => s.showConfirmClear)
  const savedToast = useStore((s) => s.savedToast)
  const focusedId = useStore((s) => s.focusedId)
  const tiles = useStore((s) => s.tiles)
  const { initFromPersisted, toggleShortcuts, toggleConfirmClear, clearCanvas } = useStore()
  const initializedRef = useRef(false)

  // ── Sync dark class on <html> so CSS variables cascade from root ──────────
  useEffect(() => {
    document.documentElement.classList.toggle('dark', isDark)
  }, [isDark])

  // ── Init from persisted state on first mount ───────────────────────────────
  useEffect(() => {
    if (initializedRef.current) return
    initializedRef.current = true
    initFromPersisted()
  }, []) // eslint-disable-line react-hooks/exhaustive-deps

  // ── Debounced auto-save on meaningful canvas changes ──────────────────────
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | null = null

    const debouncedSave = () => {
      if (timer) clearTimeout(timer)
      timer = setTimeout(() => {
        useStore.getState().saveWorkspace(DEFAULT_WORKSPACE)
      }, AUTO_SAVE_DEBOUNCE_MS)
    }

    const unsubscribe = useStore.subscribe((state, prevState) => {
      if (
        state.tiles !== prevState.tiles ||
        state.zoom !== prevState.zoom ||
        state.panX !== prevState.panX ||
        state.panY !== prevState.panY
      ) {
        debouncedSave()
      }
    })

    return () => {
      unsubscribe()
      if (timer) clearTimeout(timer)
    }
  }, [])

  // ── Save on window close ──────────────────────────────────────────────────
  useEffect(() => {
    const handleUnload = () => {
      useStore.getState().saveWorkspace(DEFAULT_WORKSPACE)
    }
    window.addEventListener('beforeunload', handleUnload)
    return () => window.removeEventListener('beforeunload', handleUnload)
  }, [])

  // ── Update window title with focused tile name ────────────────────────────
  useEffect(() => {
    const tile = tiles.find((t) => t.id === focusedId)
    document.title = tile ? `${tile.name} — SunnyTerm` : 'SunnyTerm'
  }, [focusedId, tiles])

  return (
    <div className="w-screen h-screen flex flex-col overflow-hidden">
      {/* Toolbar */}
      <Toolbar />

      {/* Canvas */}
      <div className="flex-1 min-h-0 relative">
        <InfiniteCanvas />
        <SearchBar />
      </div>

      {/* Save toast */}
      {savedToast && (
        <div className="fixed bottom-4 right-4 bg-green-600/90 text-white text-xs font-medium px-3 py-1.5 rounded shadow-lg pointer-events-none z-[99999] transition-opacity">
          Saved ✓
        </div>
      )}

      {/* Confirm clear canvas modal */}
      {showConfirmClear && (
        <ConfirmClearModal
          tileCount={tiles.length}
          onConfirm={clearCanvas}
          onCancel={toggleConfirmClear}
        />
      )}

      {/* Keyboard shortcuts modal */}
      {showShortcuts && (
        <ShortcutsModal onClose={toggleShortcuts} />
      )}
    </div>
  )
}

// ── Toolbar ───────────────────────────────────────────────────────────────────

function Toolbar() {
  const { spawnTile, toggleMinimap, toggleSearch, undo, redo, resetView, fitAllTiles, toggleDark, zoomIn, zoomOut } = useStore()
  const undoStack = useStore((s) => s.undoStack)
  const redoStack = useStore((s) => s.redoStack)
  const showMinimap = useStore((s) => s.showMinimap)
  const isDark = useStore((s) => s.isDark)
  const zoom = useStore((s) => s.zoom)

  const ico = 14
  const btn = 'p-1.5 rounded-md text-text-muted hover:text-text-primary hover:bg-black/5 dark:hover:bg-white/8 transition-colors flex items-center gap-1.5'
  const btnLabel = 'p-1.5 px-2 rounded-md text-xs text-text-muted hover:text-text-primary hover:bg-black/5 dark:hover:bg-white/8 transition-colors flex items-center gap-1.5'
  const sep = 'w-px h-5 bg-border mx-0.5'

  return (
    <div
      className="flex items-center gap-0.5 px-3 bg-toolbar shrink-0"
      style={{ height: 44, WebkitAppRegion: 'drag' } as React.CSSProperties}
    >
      {/* macOS traffic light spacer */}
      <div style={{ width: 72 }} />

      <div className="flex items-center gap-0.5" style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}>
        <button className={btnLabel} onClick={() => spawnTile('terminal')} title="New Terminal (⌘T)">
          <Terminal size={ico} /> <span className="text-[11px]">Terminal</span>
        </button>
        <button className={btnLabel} onClick={() => spawnTile('http')} title="New HTTP Client (⌘⇧N)">
          <Globe size={ico} /> <span className="text-[11px]">HTTP</span>
        </button>
        <button className={btnLabel} onClick={() => spawnTile('postgres')} title="New PostgreSQL Client (⌘⇧P)">
          <Database size={ico} /> <span className="text-[11px]">DB</span>
        </button>

        <div className={sep} />

        <button className={btn} onClick={undo} disabled={undoStack.length === 0} title="Undo (⌘Z)">
          <Undo2 size={ico} />
        </button>
        <button className={btn} onClick={redo} disabled={redoStack.length === 0} title="Redo (⌘⇧Z)">
          <Redo2 size={ico} />
        </button>

        <div className={sep} />

        <button
          className={`${btn} ${showMinimap ? 'text-blue-400' : ''}`}
          onClick={toggleMinimap}
          title="Toggle Minimap (⌘M)"
        >
          <Map size={ico} />
        </button>
        <button className={btn} onClick={toggleSearch} title="Search (⌘F)">
          <Search size={ico} />
        </button>

        <div className={sep} />

        <button className={btn} onClick={zoomOut} title="Zoom out (⌘-)">
          <ZoomOut size={ico} />
        </button>
        <button
          className={`${btn} font-mono tabular-nums min-w-[42px] justify-center text-[11px]`}
          onClick={resetView}
          title="Reset zoom (⌘0)"
        >
          {Math.round(zoom * 100)}%
        </button>
        <button className={btn} onClick={zoomIn} title="Zoom in (⌘+)">
          <ZoomIn size={ico} />
        </button>

        <button className={btn} onClick={fitAllTiles} title="Fit all tiles">
          <Maximize size={ico} />
        </button>

        <div className={sep} />

        <button
          className={btn}
          onClick={toggleDark}
          title={isDark ? 'Light mode (⌘⇧D)' : 'Dark mode (⌘⇧D)'}
        >
          {isDark ? <Sun size={ico} /> : <Moon size={ico} />}
        </button>

        <div className={sep} />

        <WorkspacePicker />
      </div>
    </div>
  )
}

// ── Confirm clear modal ──────────────────────────────────────────────────────

function ConfirmClearModal({ tileCount, onConfirm, onCancel }: { tileCount: number; onConfirm: () => void; onCancel: () => void }) {
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onCancel()
      if (e.key === 'Enter') onConfirm()
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [onConfirm, onCancel])

  return (
    <div
      className="fixed inset-0 bg-black/50 flex items-center justify-center z-[99998]"
      onClick={onCancel}
    >
      <div
        className="bg-tile border border-border rounded-lg shadow-2xl p-5 w-80"
        onClick={(e) => e.stopPropagation()}
      >
        <h2 className="text-text-primary font-semibold text-sm mb-2">New Canvas</h2>
        <p className="text-text-muted text-xs mb-4">
          This will close all {tileCount} tile{tileCount !== 1 ? 's' : ''} and start fresh. This action cannot be undone.
        </p>
        <div className="flex justify-end gap-2">
          <button
            className="text-xs px-3 py-1.5 rounded-md border border-border text-text-muted hover:text-text-primary hover:bg-black/5 dark:hover:bg-white/8 transition-colors"
            onClick={onCancel}
          >
            Cancel
          </button>
          <button
            className="text-xs px-3 py-1.5 rounded-md bg-red-600 text-white hover:bg-red-700 transition-colors"
            onClick={onConfirm}
          >
            Clear All
          </button>
        </div>
      </div>
    </div>
  )
}

// ── Shortcuts modal ───────────────────────────────────────────────────────────

const SHORTCUTS: { key: string; desc: string }[] = [
  { key: '⌘T', desc: 'New Terminal' },
  { key: '⌘N', desc: 'New Terminal (alias)' },
  { key: '⌘⇧N', desc: 'New HTTP pane' },
  { key: '⌘⇧P', desc: 'New PostgreSQL pane' },
  { key: '⌘W', desc: 'Close focused tile' },
  { key: '⌘Z', desc: 'Undo' },
  { key: '⌘⇧Z', desc: 'Redo' },
  { key: '⌘M', desc: 'Toggle minimap' },
  { key: '⌘F', desc: 'Toggle search' },
  { key: '⌘L', desc: 'Start output linking' },
  { key: '⌘S', desc: 'Save workspace' },
  { key: '⌘0', desc: 'Reset zoom to 100%' },
  { key: '⌘⇧D', desc: 'Toggle dark / light mode' },
  { key: '⌘1–9', desc: 'Switch workspace by index' },
  { key: '⌘+/−', desc: 'Zoom in / out' },
  { key: 'Tab', desc: 'Focus next tile' },
  { key: '⇧Tab', desc: 'Focus previous tile' },
  { key: '?', desc: 'Show this help' },
  { key: 'Esc', desc: 'Cancel linking' },
  { key: 'Space+drag', desc: 'Pan canvas' },
  { key: '⌘+scroll', desc: 'Zoom canvas' },
  { key: 'Double-click', desc: 'New terminal at cursor' },
  { key: 'Right-click tile', desc: 'Context menu (rename, restart, etc.)' },
]

function ShortcutsModal({ onClose }: { onClose: () => void }) {
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [onClose])

  return (
    <div
      className="fixed inset-0 bg-black/50 flex items-center justify-center z-[99998]"
      onClick={onClose}
    >
      <div
        className="bg-tile border border-border rounded-lg shadow-2xl p-5 w-96 max-h-[80vh] overflow-y-auto"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-text-primary font-semibold text-sm">Keyboard Shortcuts</h2>
          <button
            className="text-text-muted hover:text-text-primary text-lg leading-none"
            onClick={onClose}
          >
            ×
          </button>
        </div>
        <div className="space-y-1">
          {SHORTCUTS.map(({ key, desc }) => (
            <div key={key} className="flex items-center justify-between py-0.5">
              <span className="text-text-muted text-xs">{desc}</span>
              <kbd className="font-mono text-[11px] bg-black/5 dark:bg-white/10 text-text-secondary px-1.5 py-0.5 rounded border border-border">
                {key}
              </kbd>
            </div>
          ))}
        </div>
        <p className="mt-4 text-text-muted text-[10px] text-center">Press ? or Esc to close</p>
      </div>
    </div>
  )
}
