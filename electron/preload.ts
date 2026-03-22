import { contextBridge, ipcRenderer } from 'electron'

// Expose a typed API surface to the renderer process via window.electronAPI
contextBridge.exposeInMainWorld('electronAPI', {
  // PTY operations
  ptySpawn: (id: string, shell: string, cols: number, rows: number, cwd?: string) =>
    ipcRenderer.invoke('pty:spawn', id, shell, cols, rows, cwd),

  ptyHas: (id: string) =>
    ipcRenderer.invoke('pty:has', id),

  ptyReattach: (id: string) =>
    ipcRenderer.invoke('pty:reattach', id),

  ptyWrite: (id: string, data: string) =>
    ipcRenderer.invoke('pty:write', id, data),

  ptyResize: (id: string, cols: number, rows: number) =>
    ipcRenderer.invoke('pty:resize', id, cols, rows),

  ptyKill: (id: string) =>
    ipcRenderer.invoke('pty:kill', id),

  ptyGetCwd: (id: string) =>
    ipcRenderer.invoke('pty:getCwd', id),

  // Subscribe to PTY output for a specific tile id
  onPtyData: (id: string, callback: (data: string) => void) => {
    const channel = `pty:data:${id}`
    const handler = (_event: Electron.IpcRendererEvent, data: string) => callback(data)
    ipcRenderer.on(channel, handler)
    // Return cleanup function
    return () => ipcRenderer.removeListener(channel, handler)
  },

  // Subscribe to PTY exit for a specific tile id
  onPtyExit: (id: string, callback: (code: number) => void) => {
    const channel = `pty:exit:${id}`
    const handler = (_event: Electron.IpcRendererEvent, code: number) => callback(code)
    ipcRenderer.on(channel, handler)
    return () => ipcRenderer.removeListener(channel, handler)
  },

  // Subscribe to menu actions sent from main process
  onMenuAction: (callback: (action: string) => void) => {
    const handler = (_event: Electron.IpcRendererEvent, action: string) => callback(action)
    ipcRenderer.on('menu:action', handler)
    return () => ipcRenderer.removeListener('menu:action', handler)
  },

  // Subscribe to URL open requests (from link clicks in terminals/webviews)
  onOpenUrl: (callback: (url: string) => void) => {
    const handler = (_event: Electron.IpcRendererEvent, url: string) => callback(url)
    ipcRenderer.on('open-url', handler)
    return () => ipcRenderer.removeListener('open-url', handler)
  },

  // Workspace operations
  workspaceList: () =>
    ipcRenderer.invoke('workspace:list'),

  workspaceSave: (name: string, layout: unknown) =>
    ipcRenderer.invoke('workspace:save', name, layout),

  workspaceLoad: (name: string) =>
    ipcRenderer.invoke('workspace:load', name),

  workspaceDelete: (name: string) =>
    ipcRenderer.invoke('workspace:delete', name),

  // App state (dark mode, last workspace, window bounds)
  appStateLoad: () =>
    ipcRenderer.invoke('appState:load'),

  appStateSave: (state: unknown) =>
    ipcRenderer.invoke('appState:save', state),

  // HTTP requests (via main process to avoid CORS)
  httpRequest: (opts: { method: string; url: string; headers: Record<string, string>; body: string | null }) =>
    ipcRenderer.invoke('http:request', opts),

  // Open URL in user's default browser
  openExternal: (url: string) =>
    ipcRenderer.invoke('shell:openExternal', url),

  // PostgreSQL operations (via main process using pg)
  pgConnect: (id: string, connectionString: string) =>
    ipcRenderer.invoke('pg:connect', id, connectionString),

  pgDisconnect: (id: string) =>
    ipcRenderer.invoke('pg:disconnect', id),

  pgQuery: (id: string, sql: string) =>
    ipcRenderer.invoke('pg:query', id, sql),

  // Command history
  historyLoad: () =>
    ipcRenderer.invoke('history:load'),

  historySave: (commands: string[]) =>
    ipcRenderer.invoke('history:save', commands),

  // Completions (path & git)
  completePath: (tileId: string, partial: string) =>
    ipcRenderer.invoke('completion:path', tileId, partial),

  completeGit: (tileId: string, type: 'branch' | 'remote' | 'tag', partial: string) =>
    ipcRenderer.invoke('completion:git', tileId, type, partial)
})
