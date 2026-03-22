import React, { useRef, useEffect, useState, useCallback } from 'react'
import { useStore } from '../store'
import { TerminalTile } from '../tiles/TerminalTile'
import { HttpTile } from '../tiles/HttpTile'
import { PostgresTile } from '../tiles/PostgresTile'
import { BrowserTile } from '../tiles/BrowserTile'
import { MoreHorizontal, Pencil, Copy, RotateCcw, ClipboardCopy, Link, X } from 'lucide-react'
import type { Tile } from '../types'

const TITLE_BAR_H = 36

/** Extract the creation timestamp from a tile ID like "tile-1710834569123-0" */
function tileCreatedAt(tile: Tile): number {
  const parts = tile.id.split('-')
  return parseInt(parts[1], 10) || 0
}

export function FocusView() {
  const tiles = useStore((s) => s.tiles)
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

  // Sort tiles by creation date
  const sorted = [...tiles].sort((a, b) => tileCreatedAt(a) - tileCreatedAt(b))

  const cardW = Math.round(containerW * 0.7)
  const cardH = containerH

  return (
    <div
      ref={scrollRef}
      className="w-full h-full overflow-x-auto overflow-y-hidden flex items-stretch"
      style={{ scrollSnapType: 'x mandatory' }}
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
        </div>
      </div>
    </div>
  )
}

// ── Helpers ────────────────────────────────────────────────────────────────────

function KindDot({ kind, isExited, isFocused }: { kind: Tile['kind']; isExited: boolean; isFocused: boolean }) {
  const colors = isExited
    ? 'bg-red-400/60'
    : !isFocused
      ? 'bg-black/15 dark:bg-white/20'
      : { terminal: 'bg-green-400', http: 'bg-blue-400', postgres: 'bg-purple-400', browser: 'bg-orange-400' }[kind]
  return <div className={`w-2.5 h-2.5 rounded-full shrink-0 ${colors}`} />
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
