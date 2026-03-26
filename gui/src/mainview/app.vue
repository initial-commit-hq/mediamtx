<script setup lang="ts">
import { ref, computed, nextTick } from "vue";

declare const Hls: any;

// ── State ──────────────────────────────────────────────────────────────────
const host        = ref("localhost");
const apiPort     = ref(9997);
const webrtcPort  = ref(8889);
const hlsPort     = ref(8888);

const statusText  = ref("Not connected");
const statusState = ref<"ok" | "err" | "">("");
const isConnected = ref(false);

const cols            = ref(3);
const page            = ref(0);
const allStreams       = ref<string[]>([]);
const selectedNames   = ref<string[]>([]);
const gridSlice       = ref<string[]>([]);
const showEmpty       = ref(true);
const emptyMsg        = ref("Enter your MediaMTX server address and click Connect to discover available streams.");
const showStreamBar   = ref(false);
const showLayoutPicker = ref(false);

let hlsInstances: Record<string, any> = {};
let rtcPeers: Record<string, RTCPeerConnection> = {};

const PAGE_SIZE   = 9;
const RTC_TIMEOUT = 6000;

// ── Computed ────────────────────────────────────────────────────────────────
const perPage     = computed(() => cols.value * cols.value);
const activeNames = computed(() => selectedNames.value.length ? selectedNames.value : allStreams.value);
const totalPages  = computed(() => Math.ceil(activeNames.value.length / perPage.value));
const pageNums    = computed(() => Array.from({ length: totalPages.value }, (_, i) => i));

// ── Helpers ─────────────────────────────────────────────────────────────────
function setStatus(text: string, state: "ok" | "err" | "") {
  statusText.value = text;
  statusState.value = state;
}

function useProxy() { return false; }

function hlsUrl(name: string) {
  const h = host.value.trim(), p = hlsPort.value;
  return useProxy() ? `/proxy/${h}/${p}/${name}/index.m3u8` : `http://${h}:${p}/${name}/index.m3u8`;
}

function whepUrl(name: string) {
  const h = host.value.trim(), p = webrtcPort.value;
  return useProxy() ? `/proxy/${h}/${p}/${name}/whep` : `http://${h}:${p}/${name}/whep`;
}

function apiBase() {
  const h = host.value.trim(), p = apiPort.value;
  return useProxy() ? `/proxy/${h}/${p}` : `http://${h}:${p}`;
}

function escHtml(s: string) {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

function cellId(name: string) {
  return `cell-${CSS.escape(name)}`;
}

// ── Connect ──────────────────────────────────────────────────────────────────
async function connect() {
  setStatus("Connecting…", "");
  teardownAll();

  try {
    const res = await fetch(`${apiBase()}/v3/paths/list`);
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const data = await res.json();

    const items = data.items ?? data.paths ?? [];
    allStreams.value = items
      .map((i: any) => i.name ?? i.id ?? "")
      .filter(Boolean)
      .sort();

    if (allStreams.value.length === 0) {
      setStatus("No active streams", "err");
      displayEmpty("Server reachable but no active streams found.");
      return;
    }

    setStatus(`${allStreams.value.length} stream${allStreams.value.length !== 1 ? "s" : ""}`, "ok");
    selectedNames.value = [...allStreams.value];
    page.value = 0;
    isConnected.value = true;
    showLayoutPicker.value = true;
    showStreamBar.value = allStreams.value.length > PAGE_SIZE;

    renderPage();
  } catch (err: any) {
    setStatus("Connection failed", "err");
    displayEmpty(`Could not reach MediaMTX API.\n\n${err.message}\n\nCheck host, API port (default 9997), and that the server allows CORS.`);
    console.error(err);
  }
}

async function refreshStreams() {
  await connect();
}

// ── Stream bar toggle ─────────────────────────────────────────────────────────
function toggleStream(name: string) {
  const idx = selectedNames.value.indexOf(name);
  if (idx === -1) {
    if (selectedNames.value.length >= PAGE_SIZE) {
      selectedNames.value.shift();
    }
    selectedNames.value.push(name);
  } else {
    selectedNames.value.splice(idx, 1);
  }
  page.value = 0;
  renderPage();
}

// ── Layout ───────────────────────────────────────────────────────────────────
function setLayout(c: number) {
  cols.value = c;
  page.value = 0;
  renderPage();
}

// ── Pagination ───────────────────────────────────────────────────────────────
function prevPage() {
  if (page.value > 0) { page.value--; renderPage(); }
}
function nextPage() {
  if (page.value < totalPages.value - 1) { page.value++; renderPage(); }
}
function goToPage(i: number) {
  page.value = i;
  renderPage();
}

// ── Grid rendering ────────────────────────────────────────────────────────────
function renderPage() {
  teardownAll();

  const names = activeNames.value;
  const start = page.value * perPage.value;
  const slice = names.slice(start, start + perPage.value);

  if (slice.length === 0) {
    displayEmpty("No streams selected.");
    return;
  }

  showEmpty.value = false;
  gridSlice.value = slice;

  nextTick(() => {
    slice.forEach(name => {
      const cell = document.getElementById(cellId(name));
      if (cell) attachStream(name, cell as HTMLElement);
    });
  });
}

function displayEmpty(msg: string) {
  showEmpty.value = true;
  emptyMsg.value = msg;
  gridSlice.value = [];
}

// ── Teardown ──────────────────────────────────────────────────────────────────
function teardownAll() {
  Object.values(hlsInstances).forEach(h => { try { h.destroy(); } catch (_) {} });
  Object.values(rtcPeers).forEach(p => { try { p.close(); } catch (_) {} });
  hlsInstances = {};
  rtcPeers = {};
  gridSlice.value = [];
}

// ── Video element factory ─────────────────────────────────────────────────────
function makeVideo() {
  const v = document.createElement("video");
  v.autoplay = true;
  v.muted = true;
  v.playsInline = true;
  v.controls = false;
  v.style.cssText = "width:100%;height:100%;object-fit:contain;background:#000;display:none;";
  return v;
}

function onVideoPlaying(video: HTMLVideoElement, cell: HTMLElement) {
  cell.querySelector(".cell-placeholder")?.remove();
  video.style.display = "block";
  cell.classList.add("playing");
}

// ── WebRTC (WHEP) ─────────────────────────────────────────────────────────────
function attachStreamWebRTC(name: string, cell: HTMLElement): Promise<void> {
  return new Promise((resolve, reject) => {
    const pc = new RTCPeerConnection({ iceServers: [] });
    rtcPeers[name] = pc;

    let settled = false;
    const fail = (reason: string) => {
      if (settled) return;
      settled = true;
      pc.close();
      delete rtcPeers[name];
      reject(new Error(reason));
    };

    const timer = setTimeout(() => fail("WebRTC timeout"), RTC_TIMEOUT);

    pc.addTransceiver("video", { direction: "recvonly" });
    pc.addTransceiver("audio", { direction: "recvonly" });

    const video = makeVideo();
    cell.insertBefore(video, cell.querySelector(".cell-label"));

    pc.ontrack = (evt) => {
      if (evt.track.kind === "video" && !video.srcObject) {
        video.srcObject = evt.streams[0] ?? new MediaStream([evt.track]);
        video.play().catch(() => {});
      }
    };

    video.addEventListener("playing", () => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      onVideoPlaying(video, cell);
      resolve();
    });

    pc.oniceconnectionstatechange = () => {
      const s = pc.iceConnectionState;
      if (s === "failed" || s === "disconnected" || s === "closed") fail(`ICE ${s}`);
    };

    (async () => {
      try {
        const offer = await pc.createOffer();
        await pc.setLocalDescription(offer);

        await Promise.race([
          new Promise<void>(r => {
            if (pc.iceGatheringState === "complete") { r(); return; }
            pc.addEventListener("icegatheringstatechange", function h() {
              if (pc.iceGatheringState === "complete") { pc.removeEventListener("icegatheringstatechange", h); r(); }
            });
          }),
          new Promise<void>(r => setTimeout(r, 3000)),
        ]);

        const resp = await fetch(whepUrl(name), {
          method:  "POST",
          headers: { "Content-Type": "application/sdp" },
          body:    pc.localDescription!.sdp,
        });

        if (!resp.ok) { fail(`WHEP HTTP ${resp.status}`); return; }
        const sdp = await resp.text();
        await pc.setRemoteDescription({ type: "answer", sdp });
      } catch (err: any) {
        fail(err.message);
      }
    })();
  });
}

// ── HLS fallback ──────────────────────────────────────────────────────────────
function attachStreamHLS(name: string, cell: HTMLElement) {
  const video = makeVideo();
  cell.insertBefore(video, cell.querySelector(".cell-label"));

  video.addEventListener("playing", () => onVideoPlaying(video, cell));
  video.addEventListener("error",   () => markError(cell, "Playback error"));

  if (Hls.isSupported()) {
    const hls = new Hls({
      lowLatencyMode: true,
      liveSyncDurationCount: 1,
      liveMaxLatencyDurationCount: 3,
      maxLiveSyncPlaybackRate: 1.5,
    });
    hls.loadSource(hlsUrl(name));
    hls.attachMedia(video);
    hls.on(Hls.Events.MANIFEST_PARSED, () => video.play().catch(() => {}));
    hls.on(Hls.Events.ERROR, (_: any, d: any) => {
      if (d.fatal) { markError(cell, "Stream unavailable"); hls.destroy(); delete hlsInstances[name]; }
    });
    hlsInstances[name] = hls;
  } else if (video.canPlayType("application/vnd.apple.mpegurl")) {
    video.src = hlsUrl(name);
    video.load();
  } else {
    markError(cell, "HLS not supported");
  }
}

// ── Entry point: WebRTC → HLS ─────────────────────────────────────────────────
async function attachStream(name: string, cell: HTMLElement) {
  try {
    await attachStreamWebRTC(name, cell);
  } catch (err: any) {
    console.info(`[${name}] WebRTC failed (${err.message}), falling back to HLS`);
    cell.querySelector("video")?.remove();
    if (!cell.querySelector(".cell-placeholder")) {
      const ph = document.createElement("div");
      ph.className = "cell-placeholder";
      ph.innerHTML = '<div class="cell-spinner"></div><span>HLS…</span>';
      cell.insertBefore(ph, cell.querySelector(".cell-label"));
    }
    attachStreamHLS(name, cell);
  }
}

function markError(cell: HTMLElement, msg: string) {
  const placeholder = cell.querySelector(".cell-placeholder");
  if (placeholder) {
    placeholder.innerHTML = `
      <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" style="opacity:.3"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>
      <span>${escHtml(msg)}</span>
    `;
  }
}
</script>

<template>
  <div>
    <div class="container">
      <!-- ── Header ──────────────────────────────────────────────────────────── -->
      <header>
        <div class="logo">
          <span class="logo-dot"></span>
          Camera Grid
        </div>

        <div class="controls">
          <div class="input-group">
            <label>Host</label>
            <input class="input-host" type="text" v-model="host" placeholder="192.168.1.10" @keydown.enter="connect" />
          </div>
          <div class="input-group">
            <label>API port</label>
            <input class="input-api" type="number" v-model="apiPort" min="1" max="65535" @keydown.enter="connect" />
          </div>
          <div class="input-group">
            <label>WebRTC</label>
            <input class="input-hls" type="number" v-model="webrtcPort" min="1" max="65535" @keydown.enter="connect" />
          </div>
          <div class="input-group">
            <label>HLS</label>
            <input class="input-hls" type="number" v-model="hlsPort" min="1" max="65535" @keydown.enter="connect" />
          </div>

          <button class="btn btn-primary" @click="connect">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12h14M12 5l7 7-7 7"/></svg>
            Connect
          </button>

          <button v-if="isConnected" class="btn btn-ghost" @click="refreshStreams">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="23 4 23 10 17 10"/><polyline points="1 20 1 14 7 14"/><path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"/></svg>
            Refresh
          </button>

          <div v-if="showLayoutPicker" class="layout-picker">
            <button class="layout-btn" :class="{ active: cols === 1 }" title="1 stream"  @click="setLayout(1)">1</button>
            <button class="layout-btn" :class="{ active: cols === 2 }" title="4 streams" @click="setLayout(2)">4</button>
            <button class="layout-btn" :class="{ active: cols === 3 }" title="9 streams" @click="setLayout(3)">9</button>
          </div>

          <div class="status-pill">
            <span class="status-dot" :class="statusState"></span>
            <span>{{ statusText }}</span>
          </div>
        </div>
      </header>

      <!-- ── Stream selector bar ─────────────────────────────────────────────── -->
      <div v-if="showStreamBar" id="stream-bar" style="display:flex">
        <span class="bar-label">Streams:</span>
        <span
          v-for="name in allStreams"
          :key="name"
          class="stream-tag"
          :class="{ active: selectedNames.includes(name) }"
          :title="name"
          @click="toggleStream(name)"
        >{{ name }}</span>
      </div>

      <!-- ── Empty state ─────────────────────────────────────────────────────── -->
      <div v-if="showEmpty" id="empty">
        <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="7" width="20" height="15" rx="2"/><polyline points="17 2 12 7 7 2"/></svg>
        <h2>No streams loaded</h2>
        <p>{{ emptyMsg }}</p>
      </div>

      <!-- ── Camera grid ──────────────────────────────────────────────────────── -->
      <main
        v-if="gridSlice.length > 0"
        id="grid"
        :style="{ gridTemplateColumns: `repeat(${cols}, 1fr)` }"
      >
        <div
          v-for="name in gridSlice"
          :key="name"
          class="cell"
          :id="cellId(name)"
        >
          <div class="cell-placeholder">
            <div class="cell-spinner"></div>
            <span>Connecting…</span>
          </div>
          <div class="cell-label">{{ name }}</div>
          <div class="cell-live">LIVE</div>
        </div>
      </main>

      <!-- ── Pagination ───────────────────────────────────────────────────────── -->
      <div v-if="totalPages > 1" id="pagination" style="display:flex">
        <button class="page-btn" @click="prevPage">‹</button>
        <button
          v-for="i in pageNums"
          :key="i"
          class="page-btn"
          :class="{ active: i === page }"
          @click="goToPage(i)"
        >{{ i + 1 }}</button>
        <button class="page-btn" @click="nextPage">›</button>
      </div>
    </div>
  </div>
</template>