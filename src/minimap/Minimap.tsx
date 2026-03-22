import React, { useCallback, useRef } from 'react'
import { useStore } from '../store'

const MINIMAP_W = 160
const MINIMAP_H = 110
const PADDING_FRAC = 0.15

// SVG path icons (simplified) for each tile kind
const TILE_ICONS: Record<string, string> = {
  terminal: 'M4 17l6-5-6-5M12 19h8', // terminal prompt >_
  http: 'M12 2a10 10 0 100 20 10 10 0 000-20zM2 12h20M12 2a15 15 0 014 10 15 15 0 01-4 10M12 2a15 15 0 00-4 10 15 15 0 004 10', // globe
  postgres: 'M4 7v10c0 2 4 4 8 4s8-2 8-4V7M4 7c0 2 4 4 8 4s8-2 8-4M4 7c0-2 4-4 8-4s8 2 8 4M4 12c0 2 4 4 8 4s8-2 8-4', // database
}

export function Minimap() {
  const tiles = useStore((s) => s.tiles)
  const zoom = useStore((s) => s.zoom)
  const panX = useStore((s) => s.panX)
  const panY = useStore((s) => s.panY)
  const focusedId = useStore((s) => s.focusedId)
  const isDark = useStore((s) => s.isDark)
  const { setPan } = useStore()

  const svgRef = useRef<SVGSVGElement>(null)
  const isDragging = useRef(false)

  // Convert a minimap pixel position to a canvas-centered pan
  const panToMiniPos = useCallback(
    (clientX: number, clientY: number, scale: number, bx: number, by: number) => {
      const rect = svgRef.current?.getBoundingClientRect()
      if (!rect) return
      const mx = clientX - rect.left
      const my = clientY - rect.top
      const canvasX = mx / scale + bx
      const canvasY = my / scale + by
      setPan(
        -(canvasX * zoom - window.innerWidth / 2),
        -(canvasY * zoom - window.innerHeight / 2)
      )
    },
    [zoom, setPan]
  )

  if (tiles.length === 0) return null

  const minX = Math.min(...tiles.map((t) => t.x))
  const minY = Math.min(...tiles.map((t) => t.y))
  const maxX = Math.max(...tiles.map((t) => t.x + t.w))
  const maxY = Math.max(...tiles.map((t) => t.y + t.h))

  const padX = (maxX - minX) * PADDING_FRAC
  const padY = (maxY - minY) * PADDING_FRAC
  const bx = minX - padX
  const by = minY - padY
  const bw = maxX - minX + padX * 2
  const bh = maxY - minY + padY * 2

  const scale = Math.min(MINIMAP_W / bw, MINIMAP_H / bh)

  const toMini = (cx: number, cy: number) => ({
    x: (cx - bx) * scale,
    y: (cy - by) * scale
  })

  const vpX = -panX / zoom
  const vpY = -panY / zoom
  const vpW = window.innerWidth / zoom
  const vpH = window.innerHeight / zoom
  const vp = toMini(vpX, vpY)
  const vpMiniW = vpW * scale
  const vpMiniH = vpH * scale

  const onPointerDown = (e: React.PointerEvent<SVGSVGElement>) => {
    e.stopPropagation()
    isDragging.current = true
    e.currentTarget.setPointerCapture(e.pointerId)
    panToMiniPos(e.clientX, e.clientY, scale, bx, by)
  }

  const onPointerMove = (e: React.PointerEvent<SVGSVGElement>) => {
    if (!isDragging.current) return
    panToMiniPos(e.clientX, e.clientY, scale, bx, by)
  }

  const onPointerUp = () => {
    isDragging.current = false
  }

  return (
    <div
      className="absolute bottom-4 right-4 rounded-xl border border-border bg-canvas/90 backdrop-blur-md overflow-hidden"
      style={{ width: MINIMAP_W, height: MINIMAP_H }}
      onPointerDown={(e) => e.stopPropagation()}
      onClick={(e) => e.stopPropagation()}
      onDoubleClick={(e) => e.stopPropagation()}
    >
      <svg
        ref={svgRef}
        width={MINIMAP_W}
        height={MINIMAP_H}
        className="cursor-crosshair"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
      >
        {/* Tiles */}
        {tiles.map((t) => {
          const pos = toMini(t.x, t.y)
          const w = Math.max(4, t.w * scale)
          const h = Math.max(4, t.h * scale)
          const isFocused = t.id === focusedId
          const iconPath = TILE_ICONS[t.kind] ?? TILE_ICONS.terminal
          const iconSize = Math.min(w, h) * 0.5
          const showIcon = iconSize >= 5

          return (
            <g key={t.id}>
              <rect
                x={pos.x}
                y={pos.y}
                width={w}
                height={h}
                rx={2}
                fill={isFocused
                  ? (isDark ? 'rgba(255,255,255,0.15)' : 'rgba(0,0,0,0.12)')
                  : (isDark ? 'rgba(255,255,255,0.08)' : 'rgba(0,0,0,0.07)')}
                stroke={isFocused
                  ? (isDark ? 'rgba(255,255,255,0.3)' : 'rgba(0,0,0,0.25)')
                  : (isDark ? 'rgba(255,255,255,0.1)' : 'rgba(0,0,0,0.12)')}
                strokeWidth={0.5}
              />
              {showIcon && (
                <g transform={`translate(${pos.x + w / 2 - iconSize / 2}, ${pos.y + h / 2 - iconSize / 2}) scale(${iconSize / 24})`}>
                  <path
                    d={iconPath}
                    fill="none"
                    stroke={isDark ? 'rgba(255,255,255,0.4)' : 'rgba(0,0,0,0.3)'}
                    strokeWidth={2}
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  />
                </g>
              )}
            </g>
          )
        })}

        {/* Viewport rect */}
        <rect
          x={vp.x}
          y={vp.y}
          width={Math.max(4, vpMiniW)}
          height={Math.max(4, vpMiniH)}
          fill="none"
          stroke={isDark ? 'rgba(255,255,255,0.4)' : 'rgba(0,0,0,0.3)'}
          strokeWidth={1}
          rx={2}
          strokeDasharray="3 2"
        />
      </svg>
    </div>
  )
}
