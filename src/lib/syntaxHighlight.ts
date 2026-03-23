import { createHighlighterCore, type HighlighterCore } from '@shikijs/core'
import { createOnigurumaEngine } from '@shikijs/engine-oniguruma'
import type { ThemeName } from './themes'

let highlighter: HighlighterCore | null = null
let initPromise: Promise<HighlighterCore> | null = null

const SHIKI_THEMES: Record<ThemeName, string> = {
  dark: 'github-dark',
  light: 'github-light',
  claude: 'monokai',
  vino: 'dracula'
}

// Only the languages we actually support — imported statically so tree-shaking works
const LANG_IMPORTS: Record<string, () => Promise<any>> = {
  javascript: () => import('@shikijs/langs/javascript'),
  typescript: () => import('@shikijs/langs/typescript'),
  tsx: () => import('@shikijs/langs/tsx'),
  jsx: () => import('@shikijs/langs/jsx'),
  json: () => import('@shikijs/langs/json'),
  jsonc: () => import('@shikijs/langs/jsonc'),
  html: () => import('@shikijs/langs/html'),
  css: () => import('@shikijs/langs/css'),
  scss: () => import('@shikijs/langs/scss'),
  less: () => import('@shikijs/langs/less'),
  bash: () => import('@shikijs/langs/bash'),
  python: () => import('@shikijs/langs/python'),
  ruby: () => import('@shikijs/langs/ruby'),
  rust: () => import('@shikijs/langs/rust'),
  go: () => import('@shikijs/langs/go'),
  java: () => import('@shikijs/langs/java'),
  kotlin: () => import('@shikijs/langs/kotlin'),
  swift: () => import('@shikijs/langs/swift'),
  c: () => import('@shikijs/langs/c'),
  cpp: () => import('@shikijs/langs/cpp'),
  csharp: () => import('@shikijs/langs/csharp'),
  php: () => import('@shikijs/langs/php'),
  lua: () => import('@shikijs/langs/lua'),
  yaml: () => import('@shikijs/langs/yaml'),
  toml: () => import('@shikijs/langs/toml'),
  xml: () => import('@shikijs/langs/xml'),
  markdown: () => import('@shikijs/langs/markdown'),
  mdx: () => import('@shikijs/langs/mdx'),
  sql: () => import('@shikijs/langs/sql'),
  graphql: () => import('@shikijs/langs/graphql'),
  dockerfile: () => import('@shikijs/langs/dockerfile'),
  makefile: () => import('@shikijs/langs/makefile'),
  vue: () => import('@shikijs/langs/vue'),
  svelte: () => import('@shikijs/langs/svelte'),
  r: () => import('@shikijs/langs/r'),
  dart: () => import('@shikijs/langs/dart'),
  zig: () => import('@shikijs/langs/zig'),
  hcl: () => import('@shikijs/langs/hcl'),
  prisma: () => import('@shikijs/langs/prisma'),
}

const THEME_IMPORTS = {
  'github-dark': () => import('@shikijs/themes/github-dark'),
  'github-light': () => import('@shikijs/themes/github-light'),
  'monokai': () => import('@shikijs/themes/monokai'),
  'dracula': () => import('@shikijs/themes/dracula'),
}

const EXT_TO_LANG: Record<string, string> = {
  js: 'javascript', jsx: 'jsx', ts: 'typescript', tsx: 'tsx',
  mjs: 'javascript', cjs: 'javascript',
  py: 'python', rb: 'ruby', rs: 'rust', go: 'go',
  java: 'java', kt: 'kotlin', swift: 'swift',
  c: 'c', cpp: 'cpp', h: 'c', hpp: 'cpp',
  cs: 'csharp', php: 'php', lua: 'lua',
  sh: 'bash', bash: 'bash', zsh: 'bash', fish: 'bash',
  json: 'json', jsonc: 'jsonc', yaml: 'yaml', yml: 'yaml', toml: 'toml',
  xml: 'xml', html: 'html', htm: 'html', svg: 'xml',
  css: 'css', scss: 'scss', less: 'less',
  md: 'markdown', mdx: 'mdx',
  sql: 'sql', graphql: 'graphql', gql: 'graphql',
  dockerfile: 'dockerfile', docker: 'dockerfile',
  makefile: 'makefile',
  vue: 'vue', svelte: 'svelte',
  r: 'r', dart: 'dart', zig: 'zig',
  tf: 'hcl', hcl: 'hcl',
  prisma: 'prisma',
  env: 'bash', gitignore: 'bash',
  txt: 'text', log: 'text', csv: 'text'
}

async function getHighlighter(): Promise<HighlighterCore> {
  if (highlighter) return highlighter
  if (initPromise) return initPromise

  initPromise = (async () => {
    // Load themes
    const themes = await Promise.all(
      Object.values(THEME_IMPORTS).map((fn) => fn().then((m) => m.default))
    )

    // Load core langs for fast startup
    const coreLangs = ['javascript', 'typescript', 'tsx', 'jsx', 'json', 'html', 'css', 'bash', 'python', 'markdown']
    const langs = await Promise.all(
      coreLangs.map((l) => LANG_IMPORTS[l]().then((m) => m.default))
    )

    const h = await createHighlighterCore({
      themes,
      langs,
      engine: createOnigurumaEngine(() => import('shiki/wasm'))
    })

    return h
  })()

  highlighter = await initPromise
  return highlighter
}

export function getLanguageFromFilename(filename: string): string {
  const lower = filename.toLowerCase()
  const base = lower.split('/').pop() || ''
  if (base === 'makefile') return 'makefile'
  if (base === 'dockerfile') return 'dockerfile'
  if (base === '.gitignore' || base === '.env') return 'bash'

  const ext = lower.split('.').pop() || ''
  return EXT_TO_LANG[ext] || 'text'
}

function escapeHtml(code: string): string {
  return code
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
}

export async function highlightCode(
  code: string,
  lang: string,
  appTheme: ThemeName
): Promise<string> {
  if (lang === 'text' || code.length > 100_000) {
    return `<pre style="margin:0;white-space:pre-wrap;word-break:break-all;">${escapeHtml(code)}</pre>`
  }

  try {
    const h = await getHighlighter()
    const theme = SHIKI_THEMES[appTheme] || 'github-dark'

    // Lazy-load language if not yet loaded
    const loadedLangs = h.getLoadedLanguages()
    if (!loadedLangs.includes(lang)) {
      const importer = LANG_IMPORTS[lang]
      if (!importer) {
        return `<pre style="margin:0;white-space:pre-wrap;word-break:break-all;">${escapeHtml(code)}</pre>`
      }
      try {
        const mod = await importer()
        await h.loadLanguage(mod.default)
      } catch {
        return `<pre style="margin:0;white-space:pre-wrap;word-break:break-all;">${escapeHtml(code)}</pre>`
      }
    }

    return h.codeToHtml(code, { lang, theme })
  } catch (err) {
    console.warn('[syntaxHighlight] failed:', err)
    return `<pre style="margin:0;white-space:pre-wrap;word-break:break-all;">${escapeHtml(code)}</pre>`
  }
}
