import React, { useState, useRef, useEffect, useCallback } from 'react'
import ReactDOM from 'react-dom'
import { useStore } from '../store'

interface Props {
  tileId: string
}

export function TerminalShortcuts({ tileId }: Props) {
  const shortcuts = useStore((s) => s.terminalShortcuts)
  const setTerminalShortcuts = useStore((s) => s.setTerminalShortcuts)

  const [adding, setAdding] = useState(false)
  const [newLabel, setNewLabel] = useState('')
  const [newCommand, setNewCommand] = useState('')
  const [contextMenu, setContextMenu] = useState<{ index: number, x: number, y: number } | null>(null)
  const [editing, setEditing] = useState<{ index: number, label: string, command: string, x: number, y: number } | null>(null)
  const [killHover, setKillHover] = useState(false)

  const containerRef = useRef<HTMLDivElement>(null)
  const addMenuRef = useRef<HTMLDivElement>(null)
  const addBtnRef = useRef<HTMLButtonElement>(null)
  const contextMenuRef = useRef<HTMLDivElement>(null)
  const editMenuRef = useRef<HTMLDivElement>(null)

  // Native contextmenu with capture:true to intercept before xterm
  useEffect(() => {
    const el = containerRef.current
    if (!el) return
    const handler = (e: MouseEvent) => {
      const btn = (e.target as HTMLElement).closest('[data-shortcut-index]') as HTMLElement | null
      if (!btn) return
      e.preventDefault()
      e.stopPropagation()
      const index = parseInt(btn.dataset.shortcutIndex!, 10)
      setContextMenu({ index, x: e.clientX, y: e.clientY })
    }
    el.addEventListener('contextmenu', handler, true)
    return () => el.removeEventListener('contextmenu', handler, true)
  }, [])

  // Close add form on outside click
  useEffect(() => {
    if (!adding) return
    const handle = (e: MouseEvent) => {
      if (addMenuRef.current && !addMenuRef.current.contains(e.target as Node) &&
          addBtnRef.current && !addBtnRef.current.contains(e.target as Node)) {
        setAdding(false)
      }
    }
    document.addEventListener('mousedown', handle)
    return () => document.removeEventListener('mousedown', handle)
  }, [adding])

  // Close context menu on outside click
  useEffect(() => {
    if (!contextMenu) return
    const handle = (e: MouseEvent) => {
      if (contextMenuRef.current && !contextMenuRef.current.contains(e.target as Node)) {
        setContextMenu(null)
      }
    }
    document.addEventListener('mousedown', handle)
    return () => document.removeEventListener('mousedown', handle)
  }, [contextMenu])

  // Close edit form on outside click
  useEffect(() => {
    if (!editing) return
    const handle = (e: MouseEvent) => {
      if (editMenuRef.current && !editMenuRef.current.contains(e.target as Node)) {
        setEditing(null)
      }
    }
    document.addEventListener('mousedown', handle)
    return () => document.removeEventListener('mousedown', handle)
  }, [editing])

  const runShortcut = useCallback((command: string) => {
    window.electronAPI.ptyWrite(tileId, command + '\n')
  }, [tileId])

  const handleAdd = useCallback(() => {
    if (!newLabel.trim() || !newCommand.trim()) return
    setTerminalShortcuts([...shortcuts, { label: newLabel.trim(), command: newCommand.trim() }])
    setNewLabel('')
    setNewCommand('')
    setAdding(false)
  }, [shortcuts, newLabel, newCommand, setTerminalShortcuts])

  const handleRemove = useCallback((index: number) => {
    setTerminalShortcuts(shortcuts.filter((_, i) => i !== index))
  }, [shortcuts, setTerminalShortcuts])

  const handleEditSave = useCallback(() => {
    if (!editing) return
    if (!editing.label.trim() || !editing.command.trim()) return
    setTerminalShortcuts(shortcuts.map((s, i) =>
      i === editing.index ? { label: editing.label.trim(), command: editing.command.trim() } : s
    ))
    setEditing(null)
  }, [editing, shortcuts, setTerminalShortcuts])

  const btnStyle = {
    color: 'var(--text-muted)',
    background: 'var(--titlebar)',
  }

  const btnClass = 'flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium cursor-pointer backdrop-blur-sm transition-all duration-150'

  return (
    <div
      ref={containerRef}
      className="absolute bottom-1 left-1.5 right-1.5 z-50 flex items-center"
      onMouseDown={e => e.stopPropagation()}
      onPointerDown={e => e.stopPropagation()}
    >
      {/* Kill button — left side, red on hover */}
      <button
        onClick={() => window.electronAPI.ptyWrite(tileId, '\x03')}
        className={btnClass + ' shrink-0'}
        style={{
          color: killHover ? '#ef4444' : 'var(--text-muted)',
          background: 'var(--titlebar)',
          transition: 'color 0.15s',
        }}
        onMouseEnter={() => setKillHover(true)}
        onMouseLeave={() => setKillHover(false)}
        title="Send Ctrl+C"
      >
        <svg className="w-3 h-3" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
          <path d="M4 4l8 8M12 4l-8 8" />
        </svg>
        <span>Kill</span>
      </button>

      {/* Divider */}
      <div className="mx-1.5 h-3 w-px shrink-0" style={{ background: 'var(--border)' }} />

      {/* Shortcuts + add button */}
      <div className="flex items-center gap-1 flex-wrap flex-1">
        {shortcuts.map((s, i) => (
          <button
            key={i}
            data-shortcut-index={i}
            className={btnClass}
            style={btnStyle}
            onClick={() => runShortcut(s.command)}
            title={`${s.command}\n(right-click to edit/delete)`}
          >
            <svg className="w-3 h-3" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
              <path d="M5 4l4 4-4 4" />
            </svg>
            <span>{s.label}</span>
          </button>
        ))}

        {/* Add shortcut button */}
        <div className="relative">
          <button
            ref={addBtnRef}
            onClick={() => setAdding((v) => !v)}
            className={btnClass}
            style={btnStyle}
            title="Add shortcut"
          >
            <svg className="w-3 h-3" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
              <path d="M8 3v10M3 8h10" />
            </svg>
          </button>

          {adding && (
            <div
              ref={addMenuRef}
              className="absolute bottom-full left-0 mb-1 min-w-[220px] rounded-lg overflow-hidden shadow-lg"
              style={{
                background: 'var(--surface)',
                border: '1px solid var(--border)',
              }}
            >
              <div className="p-2 flex flex-col gap-1.5">
                <input
                  className="w-full px-2 py-1 rounded text-[11px] border-none outline-none"
                  style={{ background: 'var(--titlebar)', color: 'var(--text-primary)' }}
                  placeholder="Label"
                  value={newLabel}
                  onChange={(e) => setNewLabel(e.target.value)}
                  onKeyDown={(e) => e.key === 'Enter' && handleAdd()}
                  autoFocus
                />
                <input
                  className="w-full px-2 py-1 rounded text-[11px] font-mono border-none outline-none"
                  style={{ background: 'var(--titlebar)', color: 'var(--text-primary)' }}
                  placeholder="Command"
                  value={newCommand}
                  onChange={(e) => setNewCommand(e.target.value)}
                  onKeyDown={(e) => e.key === 'Enter' && handleAdd()}
                />
                <div className="flex gap-1.5 justify-end">
                  <button
                    className="px-2 py-0.5 rounded text-[10px] cursor-pointer transition-colors"
                    style={{ color: 'var(--text-muted)' }}
                    onClick={() => setAdding(false)}
                  >
                    Cancel
                  </button>
                  <button
                    className="px-2 py-0.5 rounded text-[10px] bg-blue-600 text-white hover:bg-blue-500 cursor-pointer transition-colors"
                    onClick={handleAdd}
                  >
                    Add
                  </button>
                </div>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Right-click context menu — portal to escape CSS transforms */}
      {contextMenu && ReactDOM.createPortal(
        <div
          ref={contextMenuRef}
          className="fixed z-[9999] rounded-lg overflow-hidden shadow-xl"
          style={{
            left: contextMenu.x,
            top: contextMenu.y,
            background: 'var(--surface)',
            border: '1px solid var(--border)',
            minWidth: 120,
          }}
          onMouseDown={e => e.stopPropagation()}
        >
          <button
            className="w-full flex items-center gap-2 px-3 py-1.5 text-[11px] cursor-pointer transition-colors hover:bg-white/10 text-left"
            style={{ color: 'var(--text-primary)' }}
            onClick={() => {
              const s = shortcuts[contextMenu.index]
              setEditing({ index: contextMenu.index, label: s.label, command: s.command, x: contextMenu.x, y: contextMenu.y })
              setContextMenu(null)
            }}
          >
            <svg className="w-3 h-3" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
              <path d="M11 2l3 3-8 8H3v-3L11 2z" />
            </svg>
            Edit
          </button>
          <button
            className="w-full flex items-center gap-2 px-3 py-1.5 text-[11px] cursor-pointer transition-colors hover:bg-red-500/20 text-left"
            style={{ color: '#ef4444' }}
            onClick={() => {
              handleRemove(contextMenu.index)
              setContextMenu(null)
            }}
          >
            <svg className="w-3 h-3" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
              <path d="M3 4h10M6 4V2h4v2M5 4v9a1 1 0 001 1h4a1 1 0 001-1V4" />
            </svg>
            Delete
          </button>
        </div>,
        document.body
      )}

      {/* Edit shortcut form — also in portal */}
      {editing && ReactDOM.createPortal(
        <div
          ref={editMenuRef}
          className="fixed z-[9999] rounded-lg overflow-hidden shadow-xl"
          style={{
            left: Math.min(editing.x, window.innerWidth - 240),
            top: Math.max(editing.y - 120, 8),
            background: 'var(--surface)',
            border: '1px solid var(--border)',
            minWidth: 220,
          }}
          onMouseDown={e => e.stopPropagation()}
        >
          <div className="p-2 flex flex-col gap-1.5">
            <input
              className="w-full px-2 py-1 rounded text-[11px] border-none outline-none"
              style={{ background: 'var(--titlebar)', color: 'var(--text-primary)' }}
              placeholder="Label"
              value={editing.label}
              onChange={(e) => setEditing({ ...editing, label: e.target.value })}
              onKeyDown={(e) => e.key === 'Enter' && handleEditSave()}
              autoFocus
            />
            <input
              className="w-full px-2 py-1 rounded text-[11px] font-mono border-none outline-none"
              style={{ background: 'var(--titlebar)', color: 'var(--text-primary)' }}
              placeholder="Command"
              value={editing.command}
              onChange={(e) => setEditing({ ...editing, command: e.target.value })}
              onKeyDown={(e) => e.key === 'Enter' && handleEditSave()}
            />
            <div className="flex gap-1.5 justify-end">
              <button
                className="px-2 py-0.5 rounded text-[10px] cursor-pointer transition-colors"
                style={{ color: 'var(--text-muted)' }}
                onClick={() => setEditing(null)}
              >
                Cancel
              </button>
              <button
                className="px-2 py-0.5 rounded text-[10px] bg-blue-600 text-white hover:bg-blue-500 cursor-pointer transition-colors"
                onClick={handleEditSave}
              >
                Save
              </button>
            </div>
          </div>
        </div>,
        document.body
      )}
    </div>
  )
}
