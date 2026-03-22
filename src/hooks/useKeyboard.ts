import { useEffect } from 'react'
import { useStore } from '../store'

/**
 * Global keyboard shortcuts
 *
 * Cmd+T / Cmd+N   — new terminal tile
 * Cmd+Shift+N     — new HTTP tile
 * Cmd+Shift+P     — new PostgreSQL tile
 * Cmd+W           — close focused tile
 * Cmd+Z           — undo
 * Cmd+Shift+Z     — redo
 * Cmd+M           — toggle minimap
 * Cmd+F           — toggle search
 * Cmd+L           — start linking (focused tile → next clicked tile)
 * Cmd+S           — save current workspace
 * Cmd+0           — reset zoom to 100% and center
 * Cmd+Shift+D     — toggle dark/light mode
 * Cmd+1-9         — switch to workspace by index
 * Cmd+Q           — quit
 * Tab / Shift+Tab — cycle focus between tiles
 * ?               — show keyboard shortcuts
 * Escape          — cancel linking
 *
 * NOTE: e.ctrlKey is intentionally NOT included in the meta check so that
 * Ctrl+C / Ctrl+D / Ctrl+Z / Ctrl+L etc. pass through to the terminal.
 */
export function useKeyboard() {
  const {
    spawnTile, removeTile,
    undo, redo,
    toggleMinimap, toggleSearch,
    startLinking, cancelLinking,
    saveWorkspace, loadWorkspace,
    resetView, fitAllTiles, zoomIn, zoomOut,
    toggleDark, toggleShortcuts,
    focusedId, linkingFromId, workspaces
  } = useStore()

  // ── Keyboard shortcut handler ─────────────────────────────────────────────

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const meta = e.metaKey

      if (e.key === 'Escape') {
        if (linkingFromId) cancelLinking()
        return
      }

      // Tab / Shift+Tab — cycle tile focus (only when NOT in an input/textarea)
      if (e.key === 'Tab' && !meta && !e.ctrlKey && !e.altKey) {
        const tag = (e.target as HTMLElement).tagName
        if (tag !== 'INPUT' && tag !== 'TEXTAREA' && tag !== 'SELECT') {
          e.preventDefault()
          const tiles = useStore.getState().tiles
          if (tiles.length === 0) return
          const idx = tiles.findIndex((t) => t.id === useStore.getState().focusedId)
          const next = e.shiftKey
            ? (idx - 1 + tiles.length) % tiles.length
            : (idx + 1) % tiles.length
          useStore.getState().focusTile(tiles[next].id)
          return
        }
      }

      // Delete / Backspace — remove focused tile or all selected tiles
      if ((e.key === 'Delete' || e.key === 'Backspace') && !meta && !e.ctrlKey && !e.altKey) {
        const tag = (e.target as HTMLElement).tagName
        if (tag !== 'INPUT' && tag !== 'TEXTAREA' && tag !== 'SELECT') {
          e.preventDefault()
          const { selectedIds, clearSelection } = useStore.getState()
          if (selectedIds.length > 0) {
            selectedIds.forEach((id) => removeTile(id))
            clearSelection()
          } else if (focusedId) {
            removeTile(focusedId)
          }
          return
        }
      }

      // '?' shortcut — show keyboard shortcuts cheatsheet
      if (e.key === '?' && !meta && !e.ctrlKey && !e.altKey) {
        const tag = (e.target as HTMLElement).tagName
        if (tag !== 'INPUT' && tag !== 'TEXTAREA') {
          e.preventDefault()
          toggleShortcuts()
          return
        }
      }

      if (!meta) return

      // Cmd+1-9: switch workspace by index
      const digit = parseInt(e.key, 10)
      if (!isNaN(digit) && digit >= 1 && digit <= 9) {
        const idx = digit - 1
        if (idx < workspaces.length) {
          e.preventDefault()
          loadWorkspace(workspaces[idx])
        }
        return
      }

      // Cmd+0: reset zoom to 100%
      if (e.key === '0') {
        e.preventDefault()
        resetView()
        return
      }

      switch (e.key.toLowerCase()) {
        case 't':
          e.preventDefault()
          spawnTile('terminal')
          break
        case 'n':
          e.preventDefault()
          if (e.shiftKey) {
            spawnTile('http')
          } else {
            spawnTile('terminal')
          }
          break
        case 'p':
          if (e.shiftKey) {
            e.preventDefault()
            spawnTile('postgres')
          }
          break
        case 'w':
          e.preventDefault()
          if (focusedId) removeTile(focusedId)
          break
        case 'z':
          e.preventDefault()
          if (e.shiftKey) redo()
          else undo()
          break
        case 'm':
          e.preventDefault()
          toggleMinimap()
          break
        case 'f':
          e.preventDefault()
          toggleSearch()
          break
        case 'l':
          e.preventDefault()
          if (focusedId) startLinking(focusedId)
          break
        case 's':
          e.preventDefault()
          saveWorkspace(undefined, true)
          break
        case 'd':
          if (e.shiftKey) {
            e.preventDefault()
            toggleDark()
          }
          break
        case '=':
        case '+':
          e.preventDefault()
          zoomIn()
          break
        case '-':
          e.preventDefault()
          zoomOut()
          break
        case 'q':
          e.preventDefault()
          window.close()
          break
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [
    focusedId, linkingFromId, workspaces,
    spawnTile, removeTile, undo, redo,
    toggleMinimap, toggleSearch, startLinking, cancelLinking,
    saveWorkspace, loadWorkspace,
    resetView, fitAllTiles, zoomIn, zoomOut,
    toggleDark, toggleShortcuts
  ])

  // ── Handle menu actions from main process ─────────────────────────────────

  useEffect(() => {
    const cleanup = window.electronAPI.onMenuAction((action) => {
      switch (action) {
        case 'new-terminal': spawnTile('terminal'); break
        case 'new-canvas': useStore.getState().toggleConfirmClear(); break
        case 'close-tile': if (focusedId) removeTile(focusedId); break
        case 'save-workspace': saveWorkspace(undefined, true); break
        case 'undo': undo(); break
        case 'redo': redo(); break
        case 'toggle-minimap': toggleMinimap(); break
        case 'toggle-dark': toggleDark(); break
        case 'reset-zoom': resetView(); break
        case 'fit-tiles': fitAllTiles(); break
        case 'zoom-in': zoomIn(); break
        case 'zoom-out': zoomOut(); break
        case 'show-shortcuts': toggleShortcuts(); break
      }
    })
    return cleanup
  }, [
    focusedId,
    spawnTile, removeTile, saveWorkspace, undo, redo,
    toggleMinimap, toggleDark, resetView, fitAllTiles,
    zoomIn, zoomOut, toggleShortcuts
  ])
}
