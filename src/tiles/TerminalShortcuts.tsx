import React, { useState, useRef, useEffect, useCallback } from 'react'

export interface Shortcut {
  label: string
  command: string
}

const STORAGE_KEY = 'sunnyterm-terminal-shortcuts'

const DEFAULT_SHORTCUTS: Shortcut[] = [
  { label: 'Claude', command: 'claude' },
  { label: 'Claude YOLO', command: 'claude --dangerously-skip-permissions' }
]

function loadShortcuts(): Shortcut[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw) return JSON.parse(raw)
  } catch {}
  return DEFAULT_SHORTCUTS
}

function saveShortcuts(shortcuts: Shortcut[]) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(shortcuts))
}

interface Props {
  tileId: string
}

export function TerminalShortcuts({ tileId }: Props) {
  const [open, setOpen] = useState(false)
  const [shortcuts, setShortcuts] = useState<Shortcut[]>(loadShortcuts)
  const [adding, setAdding] = useState(false)
  const [newLabel, setNewLabel] = useState('')
  const [newCommand, setNewCommand] = useState('')
  const rootRef = useRef<HTMLDivElement>(null)

  // Close on outside click
  useEffect(() => {
    if (!open) return
    const handle = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false)
        setAdding(false)
      }
    }
    document.addEventListener('mousedown', handle)
    return () => document.removeEventListener('mousedown', handle)
  }, [open])

  const runShortcut = useCallback((command: string) => {
    window.electronAPI.ptyWrite(tileId, command + '\n')
    setOpen(false)
  }, [tileId])

  const handleAdd = useCallback(() => {
    if (!newLabel.trim() || !newCommand.trim()) return
    const updated = [...shortcuts, { label: newLabel.trim(), command: newCommand.trim() }]
    setShortcuts(updated)
    saveShortcuts(updated)
    setNewLabel('')
    setNewCommand('')
    setAdding(false)
  }, [shortcuts, newLabel, newCommand])

  const handleRemove = useCallback((index: number) => {
    const updated = shortcuts.filter((_, i) => i !== index)
    setShortcuts(updated)
    saveShortcuts(updated)
  }, [shortcuts])

  return (
    <div
      ref={rootRef}
      className="absolute bottom-1 left-1.5 z-50"
      onMouseDown={e => e.stopPropagation()}
      onPointerDown={e => e.stopPropagation()}
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => { if (!adding) setOpen(false) }}
    >
      {/* Menu — sits directly above the button with no gap via pb-1 on this wrapper */}
      {open && (
        <div
          className="pb-1"
        >
          <div
            className="min-w-[220px] rounded-lg overflow-hidden shadow-lg"
            style={{
              background: 'var(--surface)',
              border: '1px solid var(--border)',
            }}
          >
            {/* Shortcut items */}
            <div className="py-0.5">
              {shortcuts.map((s, i) => (
                <div
                  key={i}
                  className="group flex items-center px-2.5 py-[5px] cursor-pointer transition-colors hover:brightness-125"
                  style={{ color: 'var(--text-primary)' }}
                  onClick={() => runShortcut(s.command)}
                >
                  <svg className="w-3 h-3 shrink-0 mr-2" style={{ color: 'var(--text-muted)' }} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                    <path d="M5 4l4 4-4 4" />
                  </svg>
                  <span className="text-[11px] font-medium">{s.label}</span>
                  <span
                    className="ml-auto text-[10px] font-mono truncate max-w-[110px] pl-3"
                    style={{ color: 'var(--text-muted)' }}
                  >
                    {s.command}
                  </span>
                  <button
                    className="ml-1.5 opacity-0 group-hover:opacity-100 transition-opacity cursor-pointer shrink-0"
                    style={{ color: 'var(--text-muted)' }}
                    onClick={(e) => { e.stopPropagation(); handleRemove(i) }}
                    title="Remove shortcut"
                    onMouseEnter={e => (e.currentTarget.style.color = '#ef4444')}
                    onMouseLeave={e => (e.currentTarget.style.color = 'var(--text-muted)')}
                  >
                    <svg className="w-3 h-3" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
                      <path d="M4 4l8 8M12 4l-8 8" />
                    </svg>
                  </button>
                </div>
              ))}
            </div>

            {/* Divider + Add */}
            <div style={{ borderTop: '1px solid var(--border)' }}>
              {adding ? (
                <div className="p-2 flex flex-col gap-1.5">
                  <input
                    className="w-full px-2 py-1 rounded text-[11px] border-none outline-none"
                    style={{
                      background: 'var(--titlebar)',
                      color: 'var(--text-primary)',
                    }}
                    placeholder="Label"
                    value={newLabel}
                    onChange={(e) => setNewLabel(e.target.value)}
                    onKeyDown={(e) => e.key === 'Enter' && handleAdd()}
                    autoFocus
                  />
                  <input
                    className="w-full px-2 py-1 rounded text-[11px] font-mono border-none outline-none"
                    style={{
                      background: 'var(--titlebar)',
                      color: 'var(--text-primary)',
                    }}
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
              ) : (
                <button
                  className="w-full flex items-center px-2.5 py-[5px] text-[11px] text-left cursor-pointer transition-colors hover:brightness-125"
                  style={{ color: 'var(--text-muted)' }}
                  onClick={() => setAdding(true)}
                >
                  <svg className="w-3 h-3 mr-2 shrink-0" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
                    <path d="M8 3v10M3 8h10" />
                  </svg>
                  Add shortcut
                </button>
              )}
            </div>
          </div>
        </div>
      )}

      {/* Buttons row */}
      <div className="flex items-center gap-1">
        <button
          onClick={() => { setOpen(!open); setAdding(false) }}
          className="flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium cursor-pointer backdrop-blur-sm transition-all duration-150"
          style={{
            color: 'var(--text-muted)',
            background: 'var(--titlebar)',
          }}
          title="Terminal shortcuts"
        >
          <svg className="w-3 h-3" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
            <path d="M13 1H3a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V3a2 2 0 0 0-2-2Z" />
            <path d="M4 5l3 3-3 3" />
            <path d="M9 11h3" />
          </svg>
          <span>Shortcuts</span>
        </button>

        <button
          onClick={() => window.electronAPI.ptyWrite(tileId, '\x03')}
          className="flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium cursor-pointer backdrop-blur-sm transition-all duration-150"
          style={{
            color: 'var(--text-muted)',
            background: 'var(--titlebar)',
          }}
          title="Send Ctrl+C"
        >
          <svg className="w-3 h-3" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
            <path d="M4 4l8 8M12 4l-8 8" />
          </svg>
          <span>Kill</span>
        </button>
      </div>
    </div>
  )
}
