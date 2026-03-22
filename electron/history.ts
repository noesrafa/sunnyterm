import * as fs from 'fs'
import * as path from 'path'
import * as os from 'os'

const SUNNYTERM_DIR = path.join(os.homedir(), '.sunnyterm-electron')
const HISTORY_FILE = path.join(SUNNYTERM_DIR, 'command-history.json')
const MAX_ENTRIES = 5000

export class HistoryManager {
  load(): string[] {
    try {
      const data = fs.readFileSync(HISTORY_FILE, 'utf8')
      const parsed = JSON.parse(data)
      return Array.isArray(parsed) ? parsed : []
    } catch {
      return []
    }
  }

  save(commands: string[]): void {
    try {
      fs.mkdirSync(SUNNYTERM_DIR, { recursive: true })
      const trimmed = commands.slice(0, MAX_ENTRIES)
      fs.writeFileSync(HISTORY_FILE, JSON.stringify(trimmed), 'utf8')
    } catch {}
  }
}
