/**
 * Input interceptor that sits between xterm's onData and ptyWrite.
 * Maintains a local buffer of the current line to drive autosuggestions
 * and completion triggers.
 */
import type { Terminal } from '@xterm/xterm'

export interface InterceptorCallbacks {
  ptyWrite: (data: string) => void
  onCommandExecuted: (command: string) => void
  getSuggestion: (prefix: string) => string | null
  renderGhostText: (text: string | null) => void
  requestCompletions: (buffer: string) => void
  dismissCompletions: () => void
}

// ANSI escape sequences we care about
const ENTER = '\r'
const BACKSPACE = '\x7f'
const CTRL_C = '\x03'
const CTRL_D = '\x04'
const CTRL_U = '\x15'
const CTRL_W = '\x17'
const CTRL_L = '\x0c'
const TAB = '\t'
const ESC = '\x1b'

// Arrow key sequences
const RIGHT_ARROW = '\x1b[C'
const LEFT_ARROW = '\x1b[D'
const UP_ARROW = '\x1b[A'
const DOWN_ARROW = '\x1b[B'
const CTRL_RIGHT = '\x1b[1;5C'  // Ctrl+Right (word forward)
const ALT_RIGHT = '\x1bf'       // Alt+Right (macOS word forward)
const ALT_LEFT = '\x1bb'        // Alt+Left (macOS word backward)

// Alternate screen buffer sequences (vim, less, etc.)
const ALT_SCREEN_ON = '\x1b[?1049h'
const ALT_SCREEN_OFF = '\x1b[?1049l'

export class InputInterceptor {
  private buffer = ''
  private isRawMode = false
  private ghostActive = false
  private currentSuggestion: string | null = null
  private completionVisible = false

  constructor(
    private terminal: Terminal,
    private cb: InterceptorCallbacks
  ) {}

  /** Handle user input from xterm.onData */
  handleInput(data: string): void {
    // In raw mode (vim, less, etc.), pass everything through
    if (this.isRawMode) {
      this.cb.ptyWrite(data)
      return
    }

    // If completion dropdown is visible, handle navigation keys
    if (this.completionVisible) {
      // Tab and arrows are handled by the CompletionDropdown component via DOM events
      // But we still need to dismiss on certain keys
      if (data === ESC || data === CTRL_C) {
        this.cb.dismissCompletions()
        this.completionVisible = false
        // Don't forward ESC to shell, it would cause issues
        return
      }
      if (data === ENTER) {
        // Enter in completion mode = select, handled by CompletionDropdown
        // Don't forward to shell
        return
      }
    }

    // Handle special keys
    if (data === ENTER) {
      this.clearGhost()
      this.cb.dismissCompletions()
      this.completionVisible = false
      const cmd = this.buffer
      this.buffer = ''
      this.cb.ptyWrite(data)
      if (cmd.trim()) {
        this.cb.onCommandExecuted(cmd)
      }
      return
    }

    if (data === BACKSPACE) {
      this.buffer = this.buffer.slice(0, -1)
      this.cb.ptyWrite(data)
      this.updateSuggestion()
      return
    }

    if (data === CTRL_C || data === CTRL_D) {
      this.buffer = ''
      this.clearGhost()
      this.cb.dismissCompletions()
      this.completionVisible = false
      this.cb.ptyWrite(data)
      return
    }

    if (data === CTRL_U) {
      this.buffer = ''
      this.clearGhost()
      this.cb.ptyWrite(data)
      return
    }

    if (data === CTRL_W) {
      // Delete last word
      const trimmed = this.buffer.trimEnd()
      const lastSpace = trimmed.lastIndexOf(' ')
      this.buffer = lastSpace === -1 ? '' : this.buffer.slice(0, lastSpace + 1)
      this.cb.ptyWrite(data)
      this.updateSuggestion()
      return
    }

    if (data === CTRL_L) {
      this.buffer = ''
      this.clearGhost()
      this.cb.ptyWrite(data)
      return
    }

    // Right arrow: accept ghost suggestion
    if (data === RIGHT_ARROW && this.ghostActive && this.currentSuggestion) {
      this.acceptSuggestion()
      return
    }

    // Ctrl+Right or Alt+Right: accept next word of suggestion
    if ((data === CTRL_RIGHT || data === ALT_RIGHT) && this.ghostActive && this.currentSuggestion) {
      this.acceptNextWord()
      return
    }

    // Tab: trigger path/git completions
    if (data === TAB) {
      this.clearGhost()
      this.cb.requestCompletions(this.buffer)
      this.completionVisible = true
      // Don't forward tab to shell — we handle it
      return
    }

    // Escape sequences
    if (data.startsWith(ESC)) {
      this.cb.ptyWrite(data)

      // Up/Down arrows: shell history navigation — buffer becomes unknown
      if (data === UP_ARROW || data === DOWN_ARROW) {
        this.buffer = ''
        this.clearGhost()
        return
      }

      // Left arrow or word-left: remove last char from buffer (approximate)
      if (data === LEFT_ARROW) {
        this.buffer = this.buffer.slice(0, -1)
        this.clearGhost()
        return
      }

      // Alt+Left (word backward): remove last word from buffer (approximate)
      if (data === ALT_LEFT) {
        const trimmed = this.buffer.trimEnd()
        const lastSpace = trimmed.lastIndexOf(' ')
        this.buffer = lastSpace === -1 ? '' : this.buffer.slice(0, lastSpace + 1)
        this.clearGhost()
        return
      }

      // Other escape sequences: pass through, keep buffer intact
      return
    }

    // Regular printable input (may be multi-character from paste)
    this.cb.dismissCompletions()
    this.completionVisible = false
    this.buffer += data
    this.cb.ptyWrite(data)
    this.updateSuggestion()
  }

  /** Handle output from PTY to detect raw mode transitions */
  handleOutput(data: string): void {
    if (data.includes(ALT_SCREEN_ON)) {
      this.isRawMode = true
      this.clearGhost()
      this.buffer = ''
    } else if (data.includes(ALT_SCREEN_OFF)) {
      this.isRawMode = false
      this.buffer = ''
    }
  }

  /** Insert a completion into the buffer and PTY */
  insertCompletion(text: string): void {
    // text is the completed portion to insert
    this.cb.ptyWrite(text)
    this.buffer += text
    this.completionVisible = false
    this.cb.dismissCompletions()
    this.updateSuggestion()
  }

  /** Get the current input buffer */
  getBuffer(): string {
    return this.buffer
  }

  dispose(): void {
    this.clearGhost()
  }

  // ── Private ──────────────────────────────────────────────────────────────────

  private updateSuggestion(): void {
    if (!this.buffer) {
      this.clearGhost()
      return
    }
    const match = this.cb.getSuggestion(this.buffer)
    if (match) {
      // Only show the part after what's already typed
      const suffix = match.slice(this.buffer.length)
      if (suffix) {
        this.currentSuggestion = match
        this.ghostActive = true
        this.cb.renderGhostText(suffix)
      } else {
        // Buffer matches the full suggestion — nothing to show
        this.clearGhost()
      }
    } else {
      this.clearGhost()
    }
  }

  private acceptSuggestion(): void {
    if (!this.currentSuggestion) return
    const suffix = this.currentSuggestion.slice(this.buffer.length)
    if (!suffix) return
    this.cb.ptyWrite(suffix)
    this.buffer = this.currentSuggestion
    this.clearGhost()
  }

  private acceptNextWord(): void {
    if (!this.currentSuggestion) return
    const suffix = this.currentSuggestion.slice(this.buffer.length)
    if (!suffix) return

    // Find next word boundary (space, /, -)
    const match = suffix.match(/^[\S]*[\s/\-]?/)
    const word = match ? match[0] : suffix

    this.cb.ptyWrite(word)
    this.buffer += word
    this.updateSuggestion()
  }

  private clearGhost(): void {
    this.ghostActive = false
    this.currentSuggestion = null
    this.cb.renderGhostText(null)
  }
}
