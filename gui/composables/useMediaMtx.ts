export const useMediaMtx = () => {
  const host       = useState('mtx-host',       () => 'localhost')
  const apiPort    = useState('mtx-api-port',   () => '9997')
  const webrtcPort = useState('mtx-webrtc-port', () => '8889')
  const hlsPort    = useState('mtx-hls-port',   () => '8888')

  // Initialise host from the page's own hostname on first client load
  if (import.meta.client && host.value === 'localhost') {
    const saved = localStorage.getItem('mtx-host')
    host.value = saved ?? window.location.hostname
  }

  // Persist settings to localStorage so they survive page reloads
  if (import.meta.client) {
    watch(host,       v => localStorage.setItem('mtx-host',        v))
    watch(apiPort,    v => localStorage.setItem('mtx-api-port',    v))
    watch(webrtcPort, v => localStorage.setItem('mtx-webrtc-port', v))
    watch(hlsPort,    v => localStorage.setItem('mtx-hls-port',    v))

    // Restore any previously saved ports
    const savedApi    = localStorage.getItem('mtx-api-port')
    const savedWrtc   = localStorage.getItem('mtx-webrtc-port')
    const savedHls    = localStorage.getItem('mtx-hls-port')
    if (savedApi)  apiPort.value    = savedApi
    if (savedWrtc) webrtcPort.value = savedWrtc
    if (savedHls)  hlsPort.value    = savedHls
  }

  // ── URL helpers ────────────────────────────────────────────────────────────
  const apiBase  = () => `http://${host.value}:${apiPort.value}`
  const hlsUrl   = (name: string) => `http://${host.value}:${hlsPort.value}/${name}/index.m3u8`
  const whepUrl  = (name: string) => `http://${host.value}:${webrtcPort.value}/${name}/whep`

  // ── Active paths ───────────────────────────────────────────────────────────
  const listPaths = async (): Promise<string[]> => {
    const r = await fetch(`${apiBase()}/v3/paths/list`)
    if (!r.ok) throw new Error(`HTTP ${r.status}`)
    const d = await r.json()
    const items: any[] = d.items ?? d.paths ?? []
    return items.map((i: any) => (i.name ?? i.id ?? '') as string).filter(Boolean).sort()
  }

  // ── Config paths (CRUD) ────────────────────────────────────────────────────
  const listConfigPaths = async (): Promise<any[]> => {
    const r = await fetch(`${apiBase()}/v3/config/paths/list`)
    if (!r.ok) throw new Error(`HTTP ${r.status}`)
    const d = await r.json()
    return d.items ?? []
  }

  const addConfigPath = async (name: string, body: object) => {
    const r = await fetch(`${apiBase()}/v3/config/paths/add/${encodeURIComponent(name)}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    })
    if (!r.ok) { const t = await r.text(); throw new Error(t || `HTTP ${r.status}`) }
  }

  const patchConfigPath = async (name: string, body: object) => {
    const r = await fetch(`${apiBase()}/v3/config/paths/patch/${encodeURIComponent(name)}`, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    })
    if (!r.ok) { const t = await r.text(); throw new Error(t || `HTTP ${r.status}`) }
  }

  const deleteConfigPath = async (name: string) => {
    const r = await fetch(`${apiBase()}/v3/config/paths/delete/${encodeURIComponent(name)}`, {
      method: 'DELETE',
    })
    if (!r.ok) { const t = await r.text(); throw new Error(t || `HTTP ${r.status}`) }
  }

  return {
    host, apiPort, webrtcPort, hlsPort,
    apiBase, hlsUrl, whepUrl,
    listPaths, listConfigPaths, addConfigPath, patchConfigPath, deleteConfigPath,
  }
}
