import React, { useEffect, useRef, useState } from 'react'

export interface CompletionItem {
  value: string
  label: string
  kind: 'file' | 'directory' | 'branch' | 'remote' | 'tag'
}

interface Props {
  items: CompletionItem[]
  position: { x: number; y: number }
  onSelect: (item: CompletionItem) => void
  onDismiss: () => void
  isDark: boolean
}

const KIND_ICONS: Record<string, string> = {
  directory: '📁',
  file: '📄',
  branch: '⎇',
  remote: '☁',
  tag: '🏷'
}

export function CompletionDropdown({ items, position, onSelect, onDismiss, isDark }: Props) {
  const [selectedIndex, setSelectedIndex] = useState(0)
  const listRef = useRef<HTMLDivElement>(null)

  // Reset selection when items change
  useEffect(() => {
    setSelectedIndex(0)
  }, [items])

  // Scroll selected item into view
  useEffect(() => {
    const list = listRef.current
    if (!list) return
    const selected = list.children[selectedIndex] as HTMLElement
    if (selected) {
      selected.scrollIntoView({ block: 'nearest' })
    }
  }, [selectedIndex])

  // Keyboard navigation
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      switch (e.key) {
        case 'ArrowDown':
          e.preventDefault()
          e.stopPropagation()
          setSelectedIndex((i) => Math.min(i + 1, items.length - 1))
          break
        case 'ArrowUp':
          e.preventDefault()
          e.stopPropagation()
          setSelectedIndex((i) => Math.max(i - 1, 0))
          break
        case 'Enter':
        case 'Tab':
          e.preventDefault()
          e.stopPropagation()
          if (items[selectedIndex]) {
            onSelect(items[selectedIndex])
          }
          break
        case 'Escape':
          e.preventDefault()
          e.stopPropagation()
          onDismiss()
          break
      }
    }
    // Capture phase to intercept before xterm gets them
    document.addEventListener('keydown', handler, true)
    return () => document.removeEventListener('keydown', handler, true)
  }, [items, selectedIndex, onSelect, onDismiss])

  if (items.length === 0) return null

  const bg = isDark ? '#2a2d31' : '#ffffff'
  const border = isDark ? '#444' : '#d0d0d0'
  const hoverBg = isDark ? '#3a3f47' : '#e8eaed'
  const textColor = isDark ? '#e0e0e0' : '#24292e'
  const dimColor = isDark ? '#888' : '#999'

  return (
    <div
      style={{
        position: 'absolute',
        left: position.x,
        top: position.y,
        zIndex: 100,
        minWidth: 200,
        maxWidth: 400,
        maxHeight: 220,
        overflowY: 'auto',
        background: bg,
        border: `1px solid ${border}`,
        borderRadius: 6,
        boxShadow: isDark
          ? '0 4px 16px rgba(0,0,0,0.5)'
          : '0 4px 16px rgba(0,0,0,0.15)',
        fontFamily: '"Google Sans Mono", Menlo, Monaco, monospace',
        fontSize: 12,
        color: textColor,
        padding: '4px 0'
      }}
      ref={listRef}
      onMouseDown={(e) => e.preventDefault()} // prevent focus loss
    >
      {items.map((item, i) => (
        <div
          key={item.value + i}
          style={{
            padding: '4px 10px',
            cursor: 'pointer',
            background: i === selectedIndex ? hoverBg : 'transparent',
            display: 'flex',
            alignItems: 'center',
            gap: 8
          }}
          onMouseEnter={() => setSelectedIndex(i)}
          onClick={() => onSelect(item)}
        >
          <span style={{ width: 16, fontSize: 13, textAlign: 'center', flexShrink: 0 }}>
            {KIND_ICONS[item.kind] || ''}
          </span>
          <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
            {item.label}
          </span>
          <span style={{ marginLeft: 'auto', color: dimColor, fontSize: 10, flexShrink: 0 }}>
            {item.kind}
          </span>
        </div>
      ))}
    </div>
  )
}
