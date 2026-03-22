import React, { useState, useRef, useCallback, useEffect } from 'react'
import { ArrowLeft, ArrowRight, RotateCw, ExternalLink, Compass } from 'lucide-react'
import { useStore } from '../store'

// ── Persistent webview registry ──────────────────────────────────────────────
// Keeps webview DOM elements alive across React unmount/remount (view switches)

interface BrowserEntry {
  webview: HTMLElement
  currentUrl: string
}

const browserRegistry = new Map<string, BrowserEntry>()

// ── Component ────────────────────────────────────────────────────────────────

interface Props {
  tileId: string
}

export function BrowserTile({ tileId }: Props) {
  const containerRef = useRef<HTMLDivElement>(null)
  const webviewRef = useRef<Electron.WebviewTag | null>(null)

  const existing = browserRegistry.get(tileId)
  const [inputValue, setInputValue] = useState(existing?.currentUrl ?? '')
  const [isLoading, setIsLoading] = useState(false)
  const [canGoBack, setCanGoBack] = useState(false)
  const [canGoForward, setCanGoForward] = useState(false)
  const [hasNavigated, setHasNavigated] = useState(!!existing)

  // Normalize URL — add protocol if missing
  const normalizeUrl = useCallback((raw: string): string => {
    const trimmed = raw.trim()
    if (!trimmed) return 'about:blank'
    if (/^https?:\/\//i.test(trimmed)) return trimmed
    if (/^localhost/i.test(trimmed) || /^127\.0\.0\.1/i.test(trimmed) || /^0\.0\.0\.0/i.test(trimmed)) {
      return `http://${trimmed}`
    }
    if (/^[\w.-]+\.\w{2,}/.test(trimmed)) return `https://${trimmed}`
    return `https://${trimmed}`
  }, [])

  const navigate = useCallback((newUrl: string) => {
    const normalized = normalizeUrl(newUrl)
    setInputValue(normalized)
    setHasNavigated(true)
    const entry = browserRegistry.get(tileId)
    if (entry) {
      entry.currentUrl = normalized
      webviewRef.current?.loadURL(normalized)
    } else {
      // Create webview on first navigation — use src attribute, not loadURL,
      // because the webview isn't ready yet right after createElement
      if (!containerRef.current) return
      const wv = document.createElement('webview') as unknown as Electron.WebviewTag
      wv.setAttribute('src', normalized)
      wv.setAttribute('allowpopups', 'true')
      wv.style.cssText = 'position:absolute;inset:0;width:100%;height:100%'
      containerRef.current.appendChild(wv as unknown as Node)
      webviewRef.current = wv
      browserRegistry.set(tileId, { webview: wv as unknown as HTMLElement, currentUrl: normalized })
    }
  }, [normalizeUrl, tileId])

  const handleSubmit = useCallback((e: React.FormEvent) => {
    e.preventDefault()
    if (inputValue.trim()) navigate(inputValue)
  }, [inputValue, navigate])

  const goBack = useCallback(() => webviewRef.current?.goBack(), [])
  const goForward = useCallback(() => webviewRef.current?.goForward(), [])
  const reload = useCallback(() => webviewRef.current?.reload(), [])
  const openExternal = useCallback(() => {
    const entry = browserRegistry.get(tileId)
    const cur = entry?.currentUrl
    if (cur && cur !== 'about:blank') window.open(cur, '_blank')
  }, [tileId])

  // ── Mount / reattach webview ──────────────────────────────────────────────

  useEffect(() => {
    if (!containerRef.current) return

    const existing = browserRegistry.get(tileId)

    if (existing) {
      // Reattach existing webview (view switch)
      containerRef.current.appendChild(existing.webview)
      webviewRef.current = existing.webview as Electron.WebviewTag
      setInputValue(existing.currentUrl)
      setHasNavigated(true)
      return () => {
        if (existing.webview.parentNode === containerRef.current) {
          containerRef.current!.removeChild(existing.webview)
        }
        webviewRef.current = null
      }
    }

    // No existing entry — empty state, webview created on first navigate
    return () => {
      const entry = browserRegistry.get(tileId)
      if (!entry) return
      const stillExists = useStore.getState().tiles.some((t) => t.id === tileId)
      if (!stillExists) browserRegistry.delete(tileId)
      if (entry.webview.parentNode === containerRef.current) {
        containerRef.current!.removeChild(entry.webview)
      }
      webviewRef.current = null
    }
  }, [tileId])

  // ── Webview event listeners ───────────────────────────────────────────────

  useEffect(() => {
    if (!hasNavigated) return
    const wv = webviewRef.current
    if (!wv) return

    const onStartLoading = () => setIsLoading(true)
    const onStopLoading = () => {
      setIsLoading(false)
      setCanGoBack(wv.canGoBack())
      setCanGoForward(wv.canGoForward())
    }
    const onNavigate = (e: Event) => {
      const navUrl = (e as any).url as string
      if (navUrl) {
        setInputValue(navUrl)
        const entry = browserRegistry.get(tileId)
        if (entry) entry.currentUrl = navUrl
      }
    }

    wv.addEventListener('did-start-loading', onStartLoading)
    wv.addEventListener('did-stop-loading', onStopLoading)
    wv.addEventListener('did-navigate', onNavigate)
    wv.addEventListener('did-navigate-in-page', onNavigate)

    return () => {
      wv.removeEventListener('did-start-loading', onStartLoading)
      wv.removeEventListener('did-stop-loading', onStopLoading)
      wv.removeEventListener('did-navigate', onNavigate)
      wv.removeEventListener('did-navigate-in-page', onNavigate)
    }
  }, [tileId, hasNavigated])

  const navBtn = 'p-1 rounded text-text-muted hover:text-text-primary hover:bg-black/5 dark:hover:bg-white/8 transition-colors disabled:opacity-30 disabled:pointer-events-none'

  return (
    <div className="w-full h-full flex flex-col">
      {/* URL bar */}
      <form
        onSubmit={handleSubmit}
        className="flex items-center gap-1 px-2 py-1.5 border-b border-border shrink-0"
      >
        <button type="button" className={navBtn} onClick={goBack} disabled={!canGoBack} title="Back">
          <ArrowLeft size={13} />
        </button>
        <button type="button" className={navBtn} onClick={goForward} disabled={!canGoForward} title="Forward">
          <ArrowRight size={13} />
        </button>
        <button type="button" className={navBtn} onClick={reload} disabled={!hasNavigated} title="Reload">
          <RotateCw size={12} className={isLoading ? 'animate-spin' : ''} />
        </button>

        <input
          type="text"
          value={inputValue}
          onChange={(e) => setInputValue(e.target.value)}
          onFocus={(e) => e.target.select()}
          autoFocus={!hasNavigated}
          className="flex-1 min-w-0 bg-black/5 dark:bg-white/6 rounded px-2 py-0.5 text-[11px] text-text-primary outline-none border border-transparent focus:border-blue-400/40 font-mono"
          placeholder="Enter URL — localhost:3000, example.com..."
          spellCheck={false}
        />

        <button type="button" className={navBtn} onClick={openExternal} disabled={!hasNavigated} title="Open in browser">
          <ExternalLink size={12} />
        </button>
      </form>

      {/* Content */}
      <div ref={containerRef} className="flex-1 min-h-0 relative">
        {!hasNavigated && <EmptyState />}
      </div>
    </div>
  )
}

// ── Empty state ──────────────────────────────────────────────────────────────

function EmptyState() {
  return (
    <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 text-text-muted">
      <Compass size={32} className="opacity-20" />
      <div className="text-center">
        <p className="text-xs font-medium text-text-secondary">No page loaded</p>
        <p className="text-[11px] mt-1 opacity-60">Type a URL above and press Enter</p>
      </div>
      <div className="flex items-center gap-2 mt-2">
        {['localhost:3000', 'localhost:5173', 'localhost:8080'].map((hint) => (
          <span
            key={hint}
            className="px-2 py-0.5 rounded bg-black/5 dark:bg-white/5 text-[10px] font-mono text-text-muted"
          >
            {hint}
          </span>
        ))}
      </div>
    </div>
  )
}
