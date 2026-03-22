import React, { useState, useRef, useCallback, useEffect } from 'react'
import { ArrowLeft, ArrowRight, RotateCw, ExternalLink } from 'lucide-react'

interface Props {
  tileId: string
}

export function BrowserTile({ tileId }: Props) {
  const [url, setUrl] = useState('http://localhost:3000')
  const [inputValue, setInputValue] = useState('http://localhost:3000')
  const [isLoading, setIsLoading] = useState(false)
  const [canGoBack, setCanGoBack] = useState(false)
  const [canGoForward, setCanGoForward] = useState(false)
  const [pageTitle, setPageTitle] = useState('')
  const webviewRef = useRef<Electron.WebviewTag | null>(null)
  const inputRef = useRef<HTMLInputElement>(null)

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
    setUrl(normalized)
    setInputValue(normalized)
    webviewRef.current?.loadURL(normalized)
  }, [normalizeUrl])

  const handleSubmit = useCallback((e: React.FormEvent) => {
    e.preventDefault()
    navigate(inputValue)
  }, [inputValue, navigate])

  const goBack = useCallback(() => webviewRef.current?.goBack(), [])
  const goForward = useCallback(() => webviewRef.current?.goForward(), [])
  const reload = useCallback(() => webviewRef.current?.reload(), [])
  const openExternal = useCallback(() => {
    if (url && url !== 'about:blank') {
      window.open(url, '_blank')
    }
  }, [url])

  // Webview event listeners
  useEffect(() => {
    const wv = webviewRef.current
    if (!wv) return

    const onStartLoading = () => setIsLoading(true)
    const onStopLoading = () => {
      setIsLoading(false)
      setCanGoBack(wv.canGoBack())
      setCanGoForward(wv.canGoForward())
    }
    const onNavigate = (e: Event & { url?: string }) => {
      const navUrl = (e as any).url as string
      if (navUrl) {
        setInputValue(navUrl)
        setUrl(navUrl)
      }
    }
    const onTitleUpdate = (e: Event & { title?: string }) => {
      setPageTitle((e as any).title || '')
    }

    wv.addEventListener('did-start-loading', onStartLoading)
    wv.addEventListener('did-stop-loading', onStopLoading)
    wv.addEventListener('did-navigate', onNavigate)
    wv.addEventListener('did-navigate-in-page', onNavigate)
    wv.addEventListener('page-title-updated', onTitleUpdate)

    return () => {
      wv.removeEventListener('did-start-loading', onStartLoading)
      wv.removeEventListener('did-stop-loading', onStopLoading)
      wv.removeEventListener('did-navigate', onNavigate)
      wv.removeEventListener('did-navigate-in-page', onNavigate)
      wv.removeEventListener('page-title-updated', onTitleUpdate)
    }
  }, [])

  const navBtn = 'p-1 rounded text-text-muted hover:text-text-primary hover:bg-black/5 dark:hover:bg-white/8 transition-colors disabled:opacity-30 disabled:pointer-events-none'

  return (
    <div className="w-full h-full flex flex-col">
      {/* URL bar */}
      <form
        onSubmit={handleSubmit}
        className="flex items-center gap-1 px-2 py-1 border-b border-border shrink-0"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <button type="button" className={navBtn} onClick={goBack} disabled={!canGoBack} title="Back">
          <ArrowLeft size={13} />
        </button>
        <button type="button" className={navBtn} onClick={goForward} disabled={!canGoForward} title="Forward">
          <ArrowRight size={13} />
        </button>
        <button type="button" className={navBtn} onClick={reload} title="Reload">
          <RotateCw size={12} className={isLoading ? 'animate-spin' : ''} />
        </button>

        <input
          ref={inputRef}
          type="text"
          value={inputValue}
          onChange={(e) => setInputValue(e.target.value)}
          onFocus={(e) => e.target.select()}
          className="flex-1 min-w-0 bg-black/5 dark:bg-white/6 rounded px-2 py-0.5 text-[11px] text-text-primary outline-none border border-transparent focus:border-blue-400/40 font-mono"
          placeholder="http://localhost:3000"
          spellCheck={false}
        />

        <button type="button" className={navBtn} onClick={openExternal} title="Open in browser">
          <ExternalLink size={12} />
        </button>
      </form>

      {/* Webview */}
      <div className="flex-1 min-h-0">
        <webview
          ref={webviewRef as any}
          src={url}
          className="w-full h-full"
          // @ts-ignore — webview attributes
          allowpopups="true"
        />
      </div>
    </div>
  )
}
