/**
 * Renderer-side command history manager.
 * Stores most-recent-first, deduped commands with persistence via IPC.
 */

const MAX_ENTRIES = 5000
let commands: string[] = []
let loaded = false
let saveTimer: ReturnType<typeof setTimeout> | null = null

export async function initHistory(): Promise<void> {
  if (loaded) return
  commands = await window.electronAPI.historyLoad()
  loaded = true
}

export function addCommand(cmd: string): void {
  const trimmed = cmd.trim()
  if (!trimmed) return

  // Remove duplicate if exists, then prepend
  const idx = commands.indexOf(trimmed)
  if (idx !== -1) commands.splice(idx, 1)
  commands.unshift(trimmed)

  // Cap size
  if (commands.length > MAX_ENTRIES) commands.length = MAX_ENTRIES

  // Debounced save
  if (saveTimer) clearTimeout(saveTimer)
  saveTimer = setTimeout(() => {
    window.electronAPI.historySave(commands)
  }, 2000)
}

/**
 * Find the most recent command that starts with the given prefix.
 * Returns the FULL command (not just the suffix).
 */
export function findMatch(prefix: string): string | null {
  if (!prefix) return null
  const lower = prefix.toLowerCase()
  for (const cmd of commands) {
    if (cmd.toLowerCase().startsWith(lower) && cmd !== prefix) {
      return cmd
    }
  }
  return null
}
