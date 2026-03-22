/**
 * Renders ghost text (autosuggestion) as a DOM overlay positioned
 * at the cursor location in the terminal.
 *
 * The overlay span is kept alive in the DOM and toggled via display
 * to avoid re-creation issues.
 */
import type { Terminal } from '@xterm/xterm'

export class GhostTextRenderer {
  private overlay: HTMLSpanElement | null = null

  constructor(
    private terminal: Terminal,
    private getIsDark: () => boolean
  ) {}

  setGhostText(text: string | null): void {
    if (!text) {
      this.hide()
      return
    }

    // Get cell dimensions from the renderer
    const core = (this.terminal as any)._core
    const dims = core?._renderService?.dimensions?.css?.cell
    if (!dims?.width || !dims?.height) return

    // Ensure overlay exists in the DOM
    this.ensureOverlay(dims.height)
    if (!this.overlay) return

    const buffer = this.terminal.buffer.active
    const cursorX = buffer.cursorX
    const cursorY = buffer.cursorY

    // Calculate pixel position relative to xterm-screen
    const pixelX = cursorX * dims.width
    const pixelY = cursorY * dims.height

    // Update content and position
    this.overlay.textContent = text
    this.overlay.style.left = `${pixelX}px`
    this.overlay.style.top = `${pixelY}px`
    this.overlay.style.height = `${dims.height}px`
    this.overlay.style.lineHeight = `${dims.height}px`
    this.overlay.style.display = ''
    this.overlay.style.color = this.getIsDark()
      ? 'rgba(255, 255, 255, 0.35)'
      : 'rgba(0, 0, 0, 0.3)'
  }

  private hide(): void {
    if (this.overlay) {
      this.overlay.style.display = 'none'
      this.overlay.textContent = ''
    }
  }

  /**
   * Ensure the overlay span exists and is attached to .xterm-screen.
   * If the span was detached (e.g. terminal re-render), re-create it.
   */
  private ensureOverlay(cellHeight: number): void {
    // Check if overlay still exists in the DOM
    if (this.overlay && this.overlay.isConnected) return

    // Find the xterm-rows element — this is where actual text rows live,
    // so our overlay aligns perfectly with terminal content
    const xtermEl = this.terminal.element
    if (!xtermEl) return
    const rows = xtermEl.querySelector('.xterm-rows') as HTMLElement
    if (!rows) return

    // Create overlay
    const span = document.createElement('span')
    span.className = 'sunnyterm-ghost-text'
    span.style.position = 'absolute'
    span.style.pointerEvents = 'none'
    span.style.zIndex = '10'
    span.style.fontFamily = '"Google Sans Mono", Menlo, Monaco, monospace'
    span.style.fontSize = '13px'
    span.style.lineHeight = `${cellHeight}px`
    span.style.whiteSpace = 'pre'
    span.style.letterSpacing = '0px'
    span.style.display = 'none'

    rows.style.position = 'relative'
    rows.appendChild(span)
    this.overlay = span
  }

  dispose(): void {
    if (this.overlay && this.overlay.parentNode) {
      this.overlay.parentNode.removeChild(this.overlay)
    }
    this.overlay = null
  }
}
