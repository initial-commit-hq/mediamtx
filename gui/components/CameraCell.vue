<template>
  <div class="cell" :class="{ playing: isPlaying }">
    <div v-if="!isPlaying" class="cell-placeholder">
      <div v-if="!errorMsg" class="cell-spinner" />
      <svg v-else width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor"
        stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" style="opacity:.3">
        <circle cx="12" cy="12" r="10"/>
        <line x1="12" y1="8" x2="12" y2="12"/>
        <line x1="12" y1="16" x2="12.01" y2="16"/>
      </svg>
      <span>{{ errorMsg || statusMsg }}</span>
    </div>
    <video ref="videoEl" autoplay muted playsinline :style="{ display: isPlaying ? 'block' : 'none' }" />
    <div class="cell-label">{{ name }}</div>
    <div class="cell-live">LIVE</div>
  </div>
</template>

<script setup lang="ts">
import Hls from 'hls.js'

const props = defineProps<{ name: string }>()

const { hlsUrl, whepUrl } = useMediaMtx()

const videoEl   = ref<HTMLVideoElement | null>(null)
const isPlaying = ref(false)
const statusMsg = ref('Connecting…')
const errorMsg  = ref('')

const RTC_TIMEOUT = 6000
let hlsInstance: Hls | null = null
let rtcPeer: RTCPeerConnection | null = null

function teardown() {
  if (hlsInstance) { try { hlsInstance.destroy() } catch (_) {} ; hlsInstance = null }
  if (rtcPeer)     { try { rtcPeer.close()        } catch (_) {} ; rtcPeer = null }
}

function onPlaying() {
  isPlaying.value = true
}

// ── WebRTC (WHEP) ────────────────────────────────────────────────────────────
function attachWebRTC(): Promise<void> {
  return new Promise((resolve, reject) => {
    const pc = new RTCPeerConnection({ iceServers: [] })
    rtcPeer = pc

    let settled = false
    const fail = (reason: string) => {
      if (settled) return
      settled = true
      pc.close(); rtcPeer = null
      reject(new Error(reason))
    }
    const timer = setTimeout(() => fail('WebRTC timeout'), RTC_TIMEOUT)

    pc.addTransceiver('video', { direction: 'recvonly' })
    pc.addTransceiver('audio', { direction: 'recvonly' })

    pc.ontrack = (evt) => {
      const v = videoEl.value
      if (!v || evt.track.kind !== 'video' || v.srcObject) return
      v.srcObject = evt.streams[0] ?? new MediaStream([evt.track])
      v.play().catch(() => {})
    }

    videoEl.value?.addEventListener('playing', () => {
      if (settled) return
      settled = true; clearTimeout(timer)
      onPlaying(); resolve()
    }, { once: true })

    pc.oniceconnectionstatechange = () => {
      const s = pc.iceConnectionState
      if (s === 'failed' || s === 'disconnected' || s === 'closed') fail(`ICE ${s}`)
    }

    ;(async () => {
      try {
        const offer = await pc.createOffer()
        await pc.setLocalDescription(offer)
        await Promise.race([
          new Promise<void>(r => {
            if (pc.iceGatheringState === 'complete') { r(); return }
            pc.addEventListener('icegatheringstatechange', function h() {
              if (pc.iceGatheringState === 'complete') {
                pc.removeEventListener('icegatheringstatechange', h); r()
              }
            })
          }),
          new Promise<void>(r => setTimeout(r, 3000)),
        ])
        const resp = await fetch(whepUrl(props.name), {
          method: 'POST',
          headers: { 'Content-Type': 'application/sdp' },
          body: pc.localDescription!.sdp,
        })
        if (!resp.ok) { fail(`WHEP HTTP ${resp.status}`); return }
        const sdp = await resp.text()
        await pc.setRemoteDescription({ type: 'answer', sdp })
      } catch (e: any) { fail(e.message) }
    })()
  })
}

// ── HLS fallback ─────────────────────────────────────────────────────────────
function attachHLS() {
  const v = videoEl.value
  if (!v) return
  if (v.srcObject) { v.srcObject = null }

  v.addEventListener('playing', onPlaying, { once: true })
  v.addEventListener('error', () => { errorMsg.value = 'Playback error' }, { once: true })

  if (Hls.isSupported()) {
    const hls = new Hls({
      lowLatencyMode: true,
      liveSyncDurationCount: 1,
      liveMaxLatencyDurationCount: 3,
      maxLiveSyncPlaybackRate: 1.5,
    })
    hls.loadSource(hlsUrl(props.name))
    hls.attachMedia(v)
    hls.on(Hls.Events.MANIFEST_PARSED, () => v.play().catch(() => {}))
    hls.on(Hls.Events.ERROR, (_: any, d: any) => {
      if (d.fatal) { errorMsg.value = 'Stream unavailable'; hls.destroy(); hlsInstance = null }
    })
    hlsInstance = hls
  } else if (v.canPlayType('application/vnd.apple.mpegurl')) {
    v.src = hlsUrl(props.name)
    v.load()
  } else {
    errorMsg.value = 'HLS not supported'
  }
}

// ── Entry point: WebRTC → HLS ─────────────────────────────────────────────────
async function attach() {
  try {
    await attachWebRTC()
  } catch (e: any) {
    console.info(`[${props.name}] WebRTC failed (${e.message}), falling back to HLS`)
    statusMsg.value = 'HLS…'
    attachHLS()
  }
}

onMounted(attach)
onUnmounted(teardown)
</script>

<style scoped>
.cell {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 10px;
  overflow: hidden;
  position: relative;
  aspect-ratio: 16/9;
  display: flex;
  align-items: center;
  justify-content: center;
}
.cell video {
  width: 100%; height: 100%;
  object-fit: contain;
  background: #000;
  display: block;
}
.cell-label {
  position: absolute; bottom: 8px; left: 8px;
  background: rgba(0, 0, 0, 0.65); color: #fff;
  font-size: 11px; font-weight: 600;
  padding: 3px 8px; border-radius: 5px;
  pointer-events: none; backdrop-filter: blur(4px);
  letter-spacing: 0.3px;
}
.cell-live {
  position: absolute; top: 8px; right: 8px;
  background: rgba(239, 68, 68, 0.85); color: #fff;
  font-size: 10px; font-weight: 700;
  padding: 2px 7px; border-radius: 4px;
  pointer-events: none; letter-spacing: 0.5px;
  display: none;
}
.cell.playing .cell-live { display: block; }
.cell-placeholder {
  display: flex; flex-direction: column;
  align-items: center; justify-content: center;
  gap: 10px; color: var(--text-dim); font-size: 12px;
}
.cell-spinner {
  width: 28px; height: 28px;
  border: 2px solid var(--border);
  border-top-color: var(--accent);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}
</style>
