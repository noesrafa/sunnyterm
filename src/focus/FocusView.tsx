import React, { useRef, useEffect, useState, useCallback } from 'react'
import { useStore } from '../store'
import { TerminalTile } from '../tiles/TerminalTile'
import { HttpTile } from '../tiles/HttpTile'
import { PostgresTile } from '../tiles/PostgresTile'
import { BrowserTile } from '../tiles/BrowserTile'
import { FileViewerTile } from '../tiles/FileViewerTile'
import { MoreHorizontal, Pencil, Copy, RotateCcw, ClipboardCopy, Link, X } from 'lucide-react'
import type { Tile } from '../types'

const TITLE_BAR_H = 36
const TAB_BAR_H = 32

/** Extract the creation timestamp from a tile ID like "tile-1710834569123-0" */
function tileCreatedAt(tile: Tile): number {
  const parts = tile.id.split('-')
  return parseInt(parts[1], 10) || 0
}

// Persistent tab order across re-renders (keyed by tile id set)
const tabOrderCache = new Map<string, string[]>()

export function FocusView() {
  const tiles = useStore((s) => s.tiles)
  const focusedId = useStore((s) => s.focusedId)
  const exitedTileIds = useStore((s) => s.exitedTileIds)
  const { focusTile } = useStore()
  const scrollRef = useRef<HTMLDivElement>(null)
  const [containerH, setContainerH] = useState(0)
  const [containerW, setContainerW] = useState(0)

  // Measure container
  useEffect(() => {
    const el = scrollRef.current
    if (!el) return
    const ro = new ResizeObserver((entries) => {
      for (const entry of entries) {
        setContainerH(entry.contentRect.height)
        setContainerW(entry.contentRect.width)
      }
    })
    ro.observe(el)
    return () => ro.disconnect()
  }, [])

  // Maintain a custom tab order that supports drag reordering
  const defaultOrder = [...tiles].sort((a, b) => tileCreatedAt(a) - tileCreatedAt(b)).map((t) => t.id)
  const tileIds = new Set(tiles.map((t) => t.id))
  const cacheKey = [...tileIds].sort().join(',')

  const [tabOrder, setTabOrder] = useState<string[]>(() => {
    return tabOrderCache.get(cacheKey) || defaultOrder
  })

  // Sync tab order when tiles are added/removed
  useEffect(() => {
    setTabOrder((prev) => {
      const existing = prev.filter((id) => tileIds.has(id))
      const newIds = defaultOrder.filter((id) => !prev.includes(id))
      const merged = [...existing, ...newIds]
      tabOrderCache.set(cacheKey, merged)
      return merged
    })
  }, [cacheKey]) // eslint-disable-line react-hooks/exhaustive-deps

  const sorted = tabOrder.map((id) => tiles.find((t) => t.id === id)!).filter(Boolean)

  const cardW = Math.round(containerW * 0.7)
  const cardH = containerH - TAB_BAR_H

  const focusedIdx = sorted.findIndex((t) => t.id === focusedId)

  const goNext = useCallback(() => {
    if (sorted.length < 2) return
    const next = (focusedIdx + 1) % sorted.length
    focusTile(sorted[next].id)
  }, [sorted, focusedIdx, focusTile])

  const goPrev = useCallback(() => {
    if (sorted.length < 2) return
    const prev = (focusedIdx - 1 + sorted.length) % sorted.length
    focusTile(sorted[prev].id)
  }, [sorted, focusedIdx, focusTile])

  // Allow trackpad horizontal scroll — convert vertical wheel to horizontal scroll
  useEffect(() => {
    const el = scrollRef.current
    if (!el) return
    const handler = (e: WheelEvent) => {
      if (Math.abs(e.deltaX) > Math.abs(e.deltaY)) return
      e.preventDefault()
      el.scrollLeft += e.deltaY
    }
    el.addEventListener('wheel', handler, { passive: false })
    return () => el.removeEventListener('wheel', handler)
  }, [])

  // Drag & drop reorder state
  const [dragTabId, setDragTabId] = useState<string | null>(null)
  const [dropTargetId, setDropTargetId] = useState<string | null>(null)
  const dropAfter = useRef(false)

  const handleDragStart = useCallback((e: React.DragEvent, tileId: string) => {
    setDragTabId(tileId)
    e.dataTransfer.effectAllowed = 'move'
    if (e.currentTarget instanceof HTMLElement) {
      e.dataTransfer.setDragImage(e.currentTarget, e.currentTarget.offsetWidth / 2, e.currentTarget.offsetHeight / 2)
    }
  }, [])

  const handleDragOver = useCallback((e: React.DragEvent, tileId: string) => {
    e.preventDefault()
    e.dataTransfer.dropEffect = 'move'
    setDropTargetId(tileId)
    // Determine if cursor is in the right half of the target → insert after
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
    dropAfter.current = e.clientX > rect.left + rect.width / 2
  }, [])

  const handleDrop = useCallback((e: React.DragEvent, targetId: string) => {
    e.preventDefault()
    if (!dragTabId || dragTabId === targetId) {
      setDragTabId(null)
      setDropTargetId(null)
      return
    }
    const movedId = dragTabId
    setTabOrder((prev) => {
      const next = prev.filter((id) => id !== movedId)
      const targetIdx = next.indexOf(targetId)
      const insertIdx = dropAfter.current ? targetIdx + 1 : targetIdx
      next.splice(insertIdx, 0, movedId)
      tabOrderCache.set(cacheKey, next)
      return next
    })
    focusTile(movedId)
    setDragTabId(null)
    setDropTargetId(null)
  }, [dragTabId, cacheKey, focusTile])

  const handleDragEnd = useCallback(() => {
    setDragTabId(null)
    setDropTargetId(null)
  }, [])

  return (
    <div className="w-full h-full flex flex-col">
      {/* Tab bar */}
      <div
        className="shrink-0 flex items-center gap-0.5 px-2 overflow-x-auto"
        style={{ height: TAB_BAR_H, scrollbarWidth: 'none' }}
      >
        {sorted.map((tile) => {
          const isFocused = tile.id === focusedId
          const isExited = exitedTileIds.includes(tile.id)
          const isDragging = dragTabId === tile.id
          const isDropTarget = dropTargetId === tile.id && dragTabId !== tile.id
          return (
            <button
              key={tile.id}
              draggable
              onDragStart={(e) => handleDragStart(e, tile.id)}
              onDragOver={(e) => handleDragOver(e, tile.id)}
              onDrop={(e) => handleDrop(e, tile.id)}
              onDragEnd={handleDragEnd}
              className={`flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-[11px] shrink-0 cursor-grab active:cursor-grabbing transition-all border ${
                isFocused
                  ? 'border-white/25 text-text-primary'
                  : 'border-border text-text-muted hover:text-text-secondary'
              } ${isDragging ? 'opacity-40' : ''} ${isDropTarget ? 'border-white/40 scale-105' : ''}`}
              onClick={() => focusTile(tile.id)}
            >
              <KindDot kind={tile.kind} isExited={isExited} isFocused={isFocused} small />
              <span className="truncate max-w-[100px]">{tile.name}</span>
            </button>
          )
        })}
      </div>

      {/* Content area with cards */}
      <div className="flex-1 min-h-0 relative">
        {/* Left nav button */}
        {sorted.length > 1 && (
          <button
            className="absolute left-3 top-1/2 -translate-y-1/2 z-20 w-8 h-8 rounded-full bg-surface/80 backdrop-blur border border-border flex items-center justify-center cursor-pointer hover:bg-surface transition-colors"
            onClick={goPrev}
          >
            <svg className="w-4 h-4 text-text-secondary" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M10 3L5 8l5 5" />
            </svg>
          </button>
        )}

        {/* Right nav button */}
        {sorted.length > 1 && (
          <button
            className="absolute right-3 top-1/2 -translate-y-1/2 z-20 w-8 h-8 rounded-full bg-surface/80 backdrop-blur border border-border flex items-center justify-center cursor-pointer hover:bg-surface transition-colors"
            onClick={goNext}
          >
            <svg className="w-4 h-4 text-text-secondary" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M6 3l5 5-5 5" />
            </svg>
          </button>
        )}

        {/* Scrollable cards */}
        <div
          ref={scrollRef}
          className="w-full h-full overflow-x-auto overflow-y-hidden flex items-stretch"
          style={{ scrollSnapType: 'x mandatory', scrollbarWidth: 'none' }}
        >
          <div className="flex items-stretch shrink-0" style={{ gap: 0 }}>
            {/* Left padding to center first card */}
            <div style={{ width: Math.round(containerW * 0.15) }} className="shrink-0" />

            {sorted.map((tile) => (
              <FocusCard
                key={tile.id}
                tile={tile}
                cardW={cardW}
                cardH={cardH}
              />
            ))}

            {/* Right padding to allow last card to scroll to center */}
            <div style={{ width: Math.round(containerW * 0.15) }} className="shrink-0" />
          </div>
        </div>
      </div>
    </div>
  )
}

// ── Focus card ────────────────────────────────────────────────────────────────

function FocusCard({ tile, cardW, cardH }: { tile: Tile; cardW: number; cardH: number }) {
  const cardRef = useRef<HTMLDivElement>(null)
  const focusedId = useStore((s) => s.focusedId)
  const exitedTileIds = useStore((s) => s.exitedTileIds)
  const { focusTile, removeTile, renameTile, spawnTile, startLinking } = useStore()
  const isFocused = focusedId === tile.id
  const isExited = exitedTileIds.includes(tile.id)

  // Scroll into view when this card becomes focused
  useEffect(() => {
    if (isFocused && cardRef.current) {
      cardRef.current.scrollIntoView({ behavior: 'smooth', inline: 'center', block: 'nearest' })
    }
  }, [isFocused])

  const [isRenaming, setIsRenaming] = useState(false)
  const [renameValue, setRenameValue] = useState(tile.name)
  const renameInputRef = useRef<HTMLInputElement>(null)

  const [ctxMenuOpen, setCtxMenuOpen] = useState(false)
  const ctxMenuRef = useRef<HTMLDivElement>(null)
  const menuBtnRef = useRef<HTMLButtonElement>(null)

  // Close context menu on outside click
  useEffect(() => {
    if (!ctxMenuOpen) return
    const handleClick = (e: MouseEvent) => {
      if (
        ctxMenuRef.current && !ctxMenuRef.current.contains(e.target as Node) &&
        menuBtnRef.current && !menuBtnRef.current.contains(e.target as Node)
      ) {
        setCtxMenuOpen(false)
      }
    }
    document.addEventListener('mousedown', handleClick)
    return () => document.removeEventListener('mousedown', handleClick)
  }, [ctxMenuOpen])

  const commitRename = useCallback(() => {
    const name = renameValue.trim()
    if (name && name !== tile.name) renameTile(tile.id, name)
    setIsRenaming(false)
  }, [renameValue, tile.id, tile.name, renameTile])

  const handleTitleDoubleClick = useCallback((e: React.MouseEvent) => {
    e.stopPropagation()
    setRenameValue(tile.name)
    setIsRenaming(true)
    setTimeout(() => renameInputRef.current?.select(), 0)
  }, [tile.name])

  const handleRenameKey = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Enter') commitRename()
    if (e.key === 'Escape') setIsRenaming(false)
  }, [commitRename])

  const handleDuplicate = useCallback(() => {
    setCtxMenuOpen(false)
    const newTile = spawnTile(tile.kind, tile.x + 30, tile.y + 30)
    if (tile.userRenamed) renameTile(newTile.id, tile.name + ' (copy)')
  }, [tile, spawnTile, renameTile])

  const handleCopyCwd = useCallback(async () => {
    setCtxMenuOpen(false)
    if (tile.kind !== 'terminal') return
    const cwd = await window.electronAPI.ptyGetCwd(tile.id)
    if (cwd) navigator.clipboard.writeText(cwd)
  }, [tile.id, tile.kind])

  const handleRestartTerminal = useCallback(() => {
    setCtxMenuOpen(false)
    document.dispatchEvent(new CustomEvent('restart-terminal', { detail: { tileId: tile.id } }))
  }, [tile.id])

  const handleLinkOutput = useCallback(() => {
    setCtxMenuOpen(false)
    startLinking(tile.id)
  }, [tile.id, startLinking])

  const handleRenameCtx = useCallback(() => {
    setCtxMenuOpen(false)
    setRenameValue(tile.name)
    setIsRenaming(true)
    setTimeout(() => renameInputRef.current?.select(), 0)
  }, [tile.name])

  const handleClose = useCallback(() => {
    setCtxMenuOpen(false)
    removeTile(tile.id)
  }, [tile.id, removeTile])

  const borderClass = isExited
    ? 'border-red-500/40'
    : 'border-border'

  const contentH = cardH - TITLE_BAR_H

  return (
    <div
      ref={cardRef}
      className="shrink-0 flex flex-col"
      style={{
        width: cardW,
        height: cardH,
        scrollSnapAlign: 'center',
        padding: '8px 4px'
      }}
      onMouseDown={() => focusTile(tile.id)}
    >
      <div
        className={[
          'flex-1 rounded-xl overflow-hidden flex flex-col',
          'bg-tile border',
          borderClass
        ].join(' ')}
      >
        {/* Title bar */}
        <div
          className="flex items-center gap-2 px-3 bg-tile shrink-0 relative"
          style={{ height: TITLE_BAR_H, userSelect: 'none' }}
          onDoubleClick={handleTitleDoubleClick}
        >
          <KindDot kind={tile.kind} isExited={isExited} isFocused={isFocused} />

          {isRenaming ? (
            <input
              ref={renameInputRef}
              value={renameValue}
              onChange={(e) => setRenameValue(e.target.value)}
              onBlur={commitRename}
              onKeyDown={handleRenameKey}
              className="flex-1 min-w-0 bg-transparent outline-none text-xs font-medium text-white/90"
              onClick={(e) => e.stopPropagation()}
              onMouseDown={(e) => e.stopPropagation()}
            />
          ) : (
            <span className="flex-1 min-w-0 truncate text-xs font-medium text-text-secondary">
              {tile.name}
            </span>
          )}

          {tile.outputLink && (
            <span className="text-yellow-400 text-xs" title="Output linked">⇒</span>
          )}

          <button
            ref={menuBtnRef}
            className="flex items-center justify-center transition-colors ml-auto"
            onClick={(e) => {
              e.stopPropagation()
              setCtxMenuOpen((v) => !v)
            }}
            onMouseDown={(e) => e.stopPropagation()}
          >
            <MoreHorizontal size={14} className="text-text-muted hover:text-text-primary" />
          </button>

          {/* Context menu */}
          {ctxMenuOpen && (
            <div
              ref={ctxMenuRef}
              style={{ position: 'absolute', right: 8, top: TITLE_BAR_H, zIndex: 99999 }}
              className="w-40 rounded border border-border bg-tile shadow-xl py-1"
              onMouseDown={(e) => e.stopPropagation()}
            >
              <CtxItem icon={<Pencil size={12} />} label="Rename" onClick={handleRenameCtx} />
              <CtxItem icon={<Copy size={12} />} label="Duplicate" onClick={handleDuplicate} />
              <div className="my-0.5 border-t border-border" />
              {tile.kind === 'terminal' && (
                <>
                  <CtxItem icon={<RotateCcw size={12} />} label="Restart" onClick={handleRestartTerminal} />
                  <CtxItem icon={<ClipboardCopy size={12} />} label="Copy CWD" onClick={handleCopyCwd} />
                </>
              )}
              <CtxItem icon={<Link size={12} />} label="Link Output" onClick={handleLinkOutput} />
              <div className="my-0.5 border-t border-border" />
              <CtxItem icon={<X size={12} />} label="Close" onClick={handleClose} danger />
            </div>
          )}
        </div>

        {/* Content */}
        <div className="flex-1 min-h-0 overflow-hidden">
          {tile.kind === 'terminal' && (
            <TerminalTile tileId={tile.id} overrideW={cardW} overrideH={contentH} />
          )}
          {tile.kind === 'http' && <HttpTile tileId={tile.id} />}
          {tile.kind === 'postgres' && <PostgresTile tileId={tile.id} />}
          {tile.kind === 'browser' && <BrowserTile tileId={tile.id} />}
          {tile.kind === 'file' && <FileViewerTile tileId={tile.id} />}
        </div>
      </div>
    </div>
  )
}

// ── Helpers ────────────────────────────────────────────────────────────────────

function KindDot({ kind, isExited, isFocused, small }: { kind: Tile['kind']; isExited: boolean; isFocused: boolean; small?: boolean }) {
  const colors = isExited
    ? 'bg-red-400/60'
    : !isFocused
      ? 'bg-black/15 dark:bg-white/20'
      : { terminal: 'bg-green-400', http: 'bg-blue-400', postgres: 'bg-purple-400', browser: 'bg-orange-400', file: 'bg-amber-400' }[kind]
  const size = small ? 'w-1.5 h-1.5' : 'w-2.5 h-2.5'
  return <div className={`${size} rounded-full shrink-0 ${colors}`} />
}

function CtxItem({ icon, label, onClick, danger }: { icon: React.ReactNode; label: string; onClick: () => void; danger?: boolean }) {
  return (
    <div
      className={`flex items-center gap-2 px-3 py-1.5 text-xs text-text-secondary hover:text-text-primary hover:bg-black/5 dark:hover:bg-white/10 cursor-pointer transition-colors ${danger ? 'hover:!text-red-400' : ''}`}
      onClick={onClick}
    >
      {icon} {label}
    </div>
  )
}
