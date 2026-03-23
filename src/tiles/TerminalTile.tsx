import React, { useEffect, useRef, useState, useCallback } from 'react'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { SearchAddon } from '@xterm/addon-search'

import '@xterm/xterm/css/xterm.css'
import { useStore } from '../store'
import { TITLE_BAR_H } from './TileContainer'
import { registerTerminal, unregisterTerminal, getTerminalEntry } from '../lib/terminalRegistry'
import type { TerminalEntry } from '../lib/terminalRegistry'
import { stripAnsi } from '../lib/stripAnsi'
import { InputInterceptor } from '../lib/inputInterceptor'
import { GhostTextRenderer } from '../lib/ghostTextRenderer'
import { initHistory, addCommand, findMatch } from '../lib/commandHistory'
import { CompletionDropdown, type CompletionItem } from './CompletionDropdown'
import { TerminalShortcuts } from './TerminalShortcuts'
import { THEMES, type ThemeName } from '../lib/themes'

// ── Component ─────────────────────────────────────────────────────────────────

interface Props {
  tileId: string
  /** Override dimensions for non-canvas views (focus mode) */
  overrideW?: number
  overrideH?: number
}

export function TerminalTile({ tileId, overrideW, overrideH }: Props) {
  const containerRef = useRef<HTMLDivElement>(null)
  const termRef = useRef<Terminal | null>(null)
  const fitAddonRef = useRef<FitAddon | null>(null)
  // Track current theme for terminal init effect (avoids re-running on theme change)
  const themeRef = useRef('dark')

  const [exitInfo, setExitInfo] = useState<{ code: number } | null>(null)
  // incrementing this forces terminal + PTY to fully reinitialise
  const [instanceKey, setInstanceKey] = useState(0)

  // Autocomplete state
  const [completionItems, setCompletionItems] = useState<CompletionItem[]>([])
  const [completionPos, setCompletionPos] = useState({ x: 0, y: 0 })
  const interceptorRef = useRef<InputInterceptor | null>(null)

  const tiles = useStore((s) => s.tiles)
  const isDark = useStore((s) => s.isDark)
  const theme = useStore((s) => s.theme)
  const zoom = useStore((s) => s.zoom)
  const focusedId = useStore((s) => s.focusedId)
  themeRef.current = theme

  const tile = tiles.find((t) => t.id === tileId)

  const outputLinkRef = useRef<string | null>(tile?.outputLink ?? null)
  outputLinkRef.current = tile?.outputLink ?? null

  const { autoRenameTile, consumeTileCwd, markTileExited, markTileAlive } = useStore()

  // ── Terminal + PTY init ────────────────────────────────────────────────────
  // On first mount: create xterm + PTY. On view switch: reattach existing xterm DOM.
  // On restart (instanceKey > 0): dispose old, create fresh.

  useEffect(() => {
    if (!containerRef.current) return

    // Check if we have a persistent xterm instance for this tile
    const existing = getTerminalEntry(tileId)

    if (existing && instanceKey === 0) {
      // ── Reattach existing xterm DOM element (view switch) ──────────────
      containerRef.current.appendChild(existing.element)
      termRef.current = existing.terminal
      fitAddonRef.current = existing.fitAddon

      // Restore exit state
      if (existing.isExited) {
        setExitInfo({ code: existing.exitCode! })
      } else {
        setExitInfo(null)
      }

      // Sync outputLink ref
      existing.outputLink = tile?.outputLink ?? null

      return () => {
        // On unmount: detach DOM but keep xterm alive
        if (existing.element.parentNode === containerRef.current) {
          containerRef.current!.removeChild(existing.element)
        }
        termRef.current = null
        fitAddonRef.current = null
      }
    }

    // ── Create new xterm instance (first mount or restart) ──────────────

    // If restarting, fully dispose the old instance
    if (instanceKey > 0) {
      const old = getTerminalEntry(tileId)
      if (old) {
        old.cleanupPty?.()
        old.cleanupExit?.()
        window.electronAPI.ptyKill(tileId)
        old.terminal.dispose()
        unregisterTerminal(tileId)
      }
    }

    setExitInfo(null)

    // Create a detached wrapper div for xterm to render into.
    // Padding lives here (not on containerRef) so xterm's mouse coordinate
    // mapping stays aligned — xterm uses getBoundingClientRect on its own
    // elements, and extra parent padding would shift the offset.
    const xtermElement = document.createElement('div')
    xtermElement.style.width = '100%'
    xtermElement.style.height = '100%'
    xtermElement.style.padding = '6px 8px'
    xtermElement.style.boxSizing = 'border-box'
    containerRef.current.appendChild(xtermElement)

    const currentTheme = THEMES[themeRef.current as ThemeName] ?? THEMES.dark
    const term = new Terminal({
      theme: currentTheme.terminal,
      fontFamily: '"Google Sans Mono", Menlo, Monaco, monospace',
      fontSize: 13,
      lineHeight: 1.0,
      cursorBlink: true,
      cursorStyle: 'block',
      scrollback: 10000,
      allowProposedApi: true,
      macOptionIsMeta: true
    })

    const fitAddon = new FitAddon()
    const webLinksAddon = new WebLinksAddon((_event, uri) => {
      window.open(uri, '_blank')
    })
    const searchAddon = new SearchAddon()

    term.loadAddon(fitAddon)
    term.loadAddon(webLinksAddon)
    term.loadAddon(searchAddon)

    term.open(xtermElement)

    // Resize terminal to match tile dimensions immediately, before PTY spawn
    const initCols = tileCols(tile?.w ?? 640)
    const initRows = tileRows(tile?.h ?? 400)
    term.resize(initCols, initRows)

    termRef.current = term
    fitAddonRef.current = fitAddon

    // Create registry entry
    const entry: TerminalEntry = {
      searchAddon,
      terminal: term,
      fitAddon,
      element: xtermElement,
      cleanupPty: null,
      cleanupExit: null,
      isExited: false,
      exitCode: null,
      savedCwd: undefined,
      outputLink: tile?.outputLink ?? null
    }
    registerTerminal(tileId, entry)

    // Determine CWD: use workspace-restored CWD on first mount, saved CWD on restart
    const cwd = instanceKey === 0
      ? (consumeTileCwd(tileId) ?? undefined)
      : entry.savedCwd

    markTileAlive(tileId)

    // ── Ghost text renderer ──────────────────────────────────────────────
    const ghostRenderer = new GhostTextRenderer(term, () => (THEMES[themeRef.current as ThemeName] ?? THEMES.dark).isDark)

    // ── Completion helper ─────────────────────────────────────────────────
    const requestCompletions = async (buffer: string) => {
      // Parse the last token from the buffer for path completion
      const tokens = buffer.trimEnd().split(/\s+/)
      const lastToken = tokens[tokens.length - 1] || ''

      // Determine completion type
      const isGitCmd = /^git\s+/.test(buffer)
      const gitSub = isGitCmd ? tokens[1] : null
      const needsBranch = gitSub && ['checkout', 'switch', 'merge', 'rebase', 'branch', 'push', 'pull', 'diff', 'log'].includes(gitSub)

      let items: CompletionItem[] = []

      if (needsBranch && tokens.length >= 3) {
        // Git branch/tag completion
        const partial = tokens[tokens.length - 1] || ''
        const [branches, tags] = await Promise.all([
          window.electronAPI.completeGit(tileId, 'branch', partial),
          window.electronAPI.completeGit(tileId, 'tag', partial)
        ])
        items = [...branches, ...tags] as CompletionItem[]
      } else if (isGitCmd && gitSub === 'remote' && tokens.length >= 3) {
        const partial = tokens[tokens.length - 1] || ''
        items = await window.electronAPI.completeGit(tileId, 'remote', partial) as CompletionItem[]
      } else {
        // Path completion for the last token
        items = await window.electronAPI.completePath(tileId, lastToken) as CompletionItem[]
      }

      if (items.length === 1) {
        // Single match: insert directly
        const completed = items[0].value
        const suffix = completed.slice(lastToken.split('/').pop()?.length || 0)
        if (suffix) {
          interceptor.insertCompletion(suffix)
        }
        setCompletionItems([])
      } else if (items.length > 1) {
        // Multiple matches: show dropdown
        // Calculate pixel position from cursor
        const core = (term as any)._core
        const dims = core?._renderService?.dimensions?.css?.cell
        const cursorX = term.buffer.active.cursorX
        const cursorY = term.buffer.active.cursorY
        if (dims) {
          setCompletionPos({
            x: cursorX * dims.width + 8, // 8px padding
            y: (cursorY + 1) * dims.height + 6 // below cursor line, 6px padding
          })
        }
        setCompletionItems(items)
      } else {
        // No matches: forward tab to shell
        setCompletionItems([])
        window.electronAPI.ptyWrite(tileId, '\t')
      }
    }

    // ── Input interceptor ─────────────────────────────────────────────────
    initHistory()
    const interceptor = new InputInterceptor(term, {
      ptyWrite: (data) => window.electronAPI.ptyWrite(tileId, data),
      onCommandExecuted: (cmd) => addCommand(cmd),
      getSuggestion: () => null,
      renderGhostText: () => {},
      requestCompletions,
      dismissCompletions: () => setCompletionItems([])
    })
    interceptorRef.current = interceptor

    // Helper to subscribe to PTY data/exit events
    const subscribePty = () => {
      const cleanup = window.electronAPI.onPtyData(tileId, (data) => {
        term.write(data)
        interceptor.handleOutput(data) // detect raw mode
        const link = entry.outputLink
        if (link) {
          const clean = stripAnsi(data)
          if (clean) window.electronAPI.ptyWrite(link, clean)
        }
      })
      entry.cleanupPty = cleanup

      const cleanupExit = window.electronAPI.onPtyExit(tileId, async (code) => {
        const cwd = await window.electronAPI.ptyGetCwd(tileId).catch(() => null)
        entry.savedCwd = cwd ?? undefined
        entry.isExited = true
        entry.exitCode = code
        setExitInfo({ code })
        markTileExited(tileId)
      })
      entry.cleanupExit = cleanupExit
    }

    // Try to reattach to an existing PTY (survives HMR), otherwise spawn new
    window.electronAPI.ptyHas(tileId).then((exists) => {
      if (exists) {
        window.electronAPI.ptyReattach(tileId).then((ok) => {
          if (ok) {
            subscribePty()
          } else {
            return window.electronAPI.ptySpawn(tileId, '', tileCols(tile?.w ?? 640), tileRows(tile?.h ?? 400), cwd).then(subscribePty)
          }
        })
      } else {
        window.electronAPI.ptySpawn(tileId, '', tileCols(tile?.w ?? 640), tileRows(tile?.h ?? 400), cwd).then(subscribePty)
      }
    }).catch((err) => {
      term.write(`\r\n\x1b[31mFailed to spawn PTY: ${err}\x1b[0m\r\n`)
    })

    // Forward user input through interceptor (handles ghost text + completions)
    term.onData((data) => {
      if (entry.isExited) return
      interceptor.handleInput(data)
    })

    term.onResize(({ cols, rows }) => {
      window.electronAPI.ptyResize(tileId, cols, rows)
    })

    // Listen for restart requests dispatched from context menu
    const handleRestartEvent = (e: Event) => {
      const { tileId: id } = (e as CustomEvent).detail
      if (id === tileId) setInstanceKey((k) => k + 1)
    }
    document.addEventListener('restart-terminal', handleRestartEvent)

    return () => {
      document.removeEventListener('restart-terminal', handleRestartEvent)
      const tileStillExists = useStore.getState().tiles.some((t) => t.id === tileId)

      if (!tileStillExists) {
        // Tile was removed — fully dispose
        interceptor.dispose()
        ghostRenderer.dispose()
        interceptorRef.current = null
        entry.cleanupPty?.()
        entry.cleanupExit?.()
        window.electronAPI.ptyKill(tileId)
        term.dispose()
        unregisterTerminal(tileId)
      } else {
        // View switch or HMR — detach DOM but keep xterm alive
        if (xtermElement.parentNode === containerRef.current) {
          containerRef.current!.removeChild(xtermElement)
        }
      }

      termRef.current = null
      fitAddonRef.current = null
    }
  }, [tileId, instanceKey]) // eslint-disable-line react-hooks/exhaustive-deps

  // ── Auto-focus terminal when this tile becomes focused ──────────────────────

  useEffect(() => {
    if (focusedId === tileId && termRef.current) {
      // Immediate focus + delayed focus to handle re-render timing
      termRef.current.focus()
      const t = setTimeout(() => termRef.current?.focus(), 50)
      return () => clearTimeout(t)
    }
  }, [focusedId, tileId])

  // ── Keep outputLink ref in sync on registry entry ──────────────────────────

  useEffect(() => {
    const entry = getTerminalEntry(tileId)
    if (entry) entry.outputLink = tile?.outputLink ?? null
  }, [tile?.outputLink, tileId])

  // ── Sync terminal theme when theme changes ──────────────────────────────

  useEffect(() => {
    const term = termRef.current
    if (!term) return
    const themeDef = THEMES[theme as ThemeName] ?? THEMES.dark
    term.options.theme = themeDef.terminal
    // Force xterm to repaint with new theme colors
    term.refresh(0, term.rows - 1)
    // Update the container background to match
    if (containerRef.current) {
      const viewport = containerRef.current.querySelector('.xterm-viewport') as HTMLElement
      if (viewport) viewport.style.backgroundColor = themeDef.terminal.background
    }
  }, [theme])

  // ── Fit terminal to tile dimensions ──────────────────────────────────────
  // Uses tile.w/tile.h directly instead of DOM measurements to avoid
  // issues with CSS transforms (zoom) and mount animations (scale 0.97→1)

  useEffect(() => {
    const term = termRef.current
    if (!term || !tile) return

    const doFit = () => {
      if (!termRef.current) return
      const core = (termRef.current as any)._core
      const dims = core?._renderService?.dimensions?.css?.cell
      if (!dims?.width || !dims?.height) {
        // Renderer not ready yet, use fitAddon as fallback
        try { fitAddonRef.current?.fit() } catch {}
        return
      }

      // Use override dimensions if provided (focus mode), otherwise tile dimensions
      const effW = overrideW ?? tile.w
      const effH = overrideH ?? tile.h
      const availW = effW - TERM_PAD_X
      const availH = effH - TITLE_BAR_H - TERM_PAD_Y
      const cols = Math.max(2, Math.floor(availW / dims.width) - 1)
      const rows = Math.max(1, Math.floor(availH / dims.height))

      if (cols !== termRef.current.cols || rows !== termRef.current.rows) {
        termRef.current.resize(cols, rows)
      }
    }

    // Staggered fits: 0ms (immediate), 100ms (after render), 200ms (after animation)
    const t1 = setTimeout(doFit, 0)
    const t2 = setTimeout(doFit, 100)
    const t3 = setTimeout(doFit, 200)

    return () => { clearTimeout(t1); clearTimeout(t2); clearTimeout(t3) }
  }, [tile?.w, tile?.h, overrideW, overrideH, zoom, tileId, instanceKey])

  const handleRestart = () => {
    setExitInfo(null)
    markTileAlive(tileId)
    setInstanceKey((k) => k + 1)
  }

  const handleCompletionSelect = useCallback((item: CompletionItem) => {
    const interceptor = interceptorRef.current
    if (!interceptor) return
    // Extract the last token from the buffer to determine what to insert
    const buffer = interceptor.getBuffer()
    const tokens = buffer.trimEnd().split(/\s+/)
    const lastToken = tokens[tokens.length - 1] || ''
    // For paths, only insert the part after the last /
    const lastSlash = lastToken.lastIndexOf('/')
    const prefix = lastSlash >= 0 ? lastToken.slice(lastSlash + 1) : lastToken
    const suffix = item.value.slice(prefix.length)
    if (suffix) interceptor.insertCompletion(suffix)
    setCompletionItems([])
  }, [])

  const handleCompletionDismiss = useCallback(() => {
    setCompletionItems([])
    // Forward tab to shell as fallback
  }, [])

  return (
    <div className="w-full h-full relative">
      <div ref={containerRef} className="w-full h-full" />

      {completionItems.length > 0 && (
        <CompletionDropdown
          items={completionItems}
          position={completionPos}
          onSelect={handleCompletionSelect}
          onDismiss={handleCompletionDismiss}
          isDark={isDark}
        />
      )}

      {exitInfo === null && <TerminalShortcuts tileId={tileId} />}

      {exitInfo !== null && (
        <div
          className="absolute inset-0 bg-black/65 flex flex-col items-center justify-center gap-3"
          onMouseDown={(e) => e.stopPropagation()}
          onPointerDown={(e) => e.stopPropagation()}
        >
          <div className="text-white/70 text-sm font-medium">
            Process exited (code {exitInfo.code})
          </div>
          <button
            className="px-4 py-1.5 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded transition-colors cursor-pointer"
            onClick={handleRestart}
          >
            ↺ Restart
          </button>
        </div>
      )}
    </div>
  )
}

// Terminal padding (must match the style on containerRef)
const TERM_PAD_X = 8 * 2 // 8px left + 8px right
const TERM_PAD_Y = 6 * 2 // 6px top + 6px bottom

// Calculate cols/rows from tile pixel dimensions and font metrics
function tileCols(tileW: number) { return Math.max(10, Math.floor((tileW - TERM_PAD_X) / 7.8) - 1) }
function tileRows(tileH: number) { return Math.max(5, Math.floor((tileH - TITLE_BAR_H - TERM_PAD_Y) / 13)) }
