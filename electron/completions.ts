import { execFile } from 'child_process'
import { readdir, stat } from 'fs/promises'
import * as path from 'path'

export interface CompletionItem {
  value: string
  label: string
  kind: 'file' | 'directory' | 'branch' | 'remote' | 'tag'
}

// ── Path completions ─────────────────────────────────────────────────────────

export async function completePath(cwd: string, partial: string): Promise<CompletionItem[]> {
  try {
    // Resolve the partial path relative to cwd
    const resolved = partial.startsWith('/')
      ? partial
      : path.join(cwd, partial)

    // Split into directory and prefix
    let dir: string
    let prefix: string

    // Check if partial ends with / — list directory contents
    if (partial.endsWith('/')) {
      dir = resolved
      prefix = ''
    } else {
      dir = path.dirname(resolved)
      prefix = path.basename(resolved)
    }

    const entries = await readdir(dir, { withFileTypes: true })
    const results: CompletionItem[] = []

    for (const entry of entries) {
      if (prefix && !entry.name.toLowerCase().startsWith(prefix.toLowerCase())) continue
      if (entry.name.startsWith('.') && !prefix.startsWith('.')) continue

      const isDir = entry.isDirectory()
      results.push({
        value: entry.name + (isDir ? '/' : ''),
        label: entry.name + (isDir ? '/' : ''),
        kind: isDir ? 'directory' : 'file'
      })

      if (results.length >= 50) break
    }

    // Sort: directories first, then alphabetically
    results.sort((a, b) => {
      if (a.kind === 'directory' && b.kind !== 'directory') return -1
      if (a.kind !== 'directory' && b.kind === 'directory') return 1
      return a.label.localeCompare(b.label)
    })

    return results
  } catch {
    return []
  }
}

// ── Git completions ──────────────────────────────────────────────────────────

function gitExec(cwd: string, args: string[]): Promise<string> {
  return new Promise((resolve, reject) => {
    execFile('git', args, { cwd, timeout: 3000 }, (err, stdout) => {
      if (err) reject(err)
      else resolve(stdout.trim())
    })
  })
}

export async function completeGit(
  cwd: string,
  type: 'branch' | 'remote' | 'tag',
  partial: string
): Promise<CompletionItem[]> {
  try {
    // Check if inside a git repo
    await gitExec(cwd, ['rev-parse', '--is-inside-work-tree'])

    let lines: string[]
    const kind = type as CompletionItem['kind']

    switch (type) {
      case 'branch':
        const raw = await gitExec(cwd, ['branch', '-a', '--format=%(refname:short)'])
        lines = raw.split('\n').filter(Boolean)
        break
      case 'remote':
        const remotes = await gitExec(cwd, ['remote'])
        lines = remotes.split('\n').filter(Boolean)
        break
      case 'tag':
        const tags = await gitExec(cwd, ['tag', '-l'])
        lines = tags.split('\n').filter(Boolean)
        break
      default:
        return []
    }

    const lowerPartial = partial.toLowerCase()
    return lines
      .filter((l) => l.toLowerCase().startsWith(lowerPartial))
      .slice(0, 30)
      .map((l) => ({ value: l, label: l, kind }))
  } catch {
    return []
  }
}
