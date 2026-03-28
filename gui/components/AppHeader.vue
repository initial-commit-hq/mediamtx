<template>
  <div>
    <header>
      <div class="logo">
        <span class="logo-dot" />
        MediaMTX
      </div>

      <div class="controls">
        <div class="input-group">
          <label>Host</label>
          <input class="input-host" v-model="host" placeholder="192.168.1.10" @keydown.enter="$emit('connect')" />
        </div>
        <div class="input-group">
          <label>API</label>
          <input class="input-port" v-model="apiPort" type="number" @keydown.enter="$emit('connect')" />
        </div>
        <div class="input-group">
          <label>WebRTC</label>
          <input class="input-port" v-model="webrtcPort" type="number" @keydown.enter="$emit('connect')" />
        </div>
        <div class="input-group">
          <label>HLS</label>
          <input class="input-port" v-model="hlsPort" type="number" @keydown.enter="$emit('connect')" />
        </div>

        <slot name="actions" />

        <div class="status-pill">
          <span class="status-dot" :class="statusClass" />
          <span>{{ statusText }}</span>
        </div>
      </div>
    </header>

    <nav class="tab-nav">
      <NuxtLink to="/" class="tab" active-class="tab-active" exact>Grid</NuxtLink>
      <NuxtLink to="/sources" class="tab" active-class="tab-active">Sources</NuxtLink>
    </nav>
  </div>
</template>

<script setup lang="ts">
defineEmits(['connect'])

defineProps<{
  statusText: string
  statusClass: string
}>()

const { host, apiPort, webrtcPort, hlsPort } = useMediaMtx()
</script>

<style scoped>
header {
  background: var(--surface);
  border-bottom: 1px solid var(--border);
  padding: 12px 20px;
  display: flex;
  align-items: center;
  gap: 16px;
  flex-wrap: wrap;
}
.logo {
  font-weight: 700;
  font-size: 15px;
  letter-spacing: -0.3px;
  display: flex;
  align-items: center;
  gap: 8px;
  white-space: nowrap;
}
.logo-dot {
  width: 8px; height: 8px;
  border-radius: 50%;
  background: var(--accent);
  flex-shrink: 0;
}
.controls {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  flex: 1;
}
.input-host { width: 180px; }
.input-port { width: 70px; }
.status-pill {
  margin-left: auto;
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--text-dim);
  white-space: nowrap;
}
.status-dot {
  width: 7px; height: 7px;
  border-radius: 50%;
  background: var(--text-dim);
  transition: background 0.3s;
}
.status-dot.ok  { background: var(--green); }
.status-dot.err { background: var(--red); }
.tab-nav {
  background: var(--surface);
  border-bottom: 1px solid var(--border);
  padding: 0 20px;
  display: flex;
  gap: 2px;
}
.tab {
  padding: 8px 16px;
  font-size: 13px;
  font-weight: 500;
  color: var(--text-dim);
  text-decoration: none;
  border-bottom: 2px solid transparent;
  margin-bottom: -1px;
  transition: color 0.15s, border-color 0.15s;
}
.tab:hover { color: var(--text); }
.tab-active { color: var(--text); border-bottom-color: var(--accent); }
</style>
