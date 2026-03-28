<template>
  <div class="page-wrap">
    <AppHeader :status-text="statusText" :status-class="statusClass" @connect="loadPaths">
      <template #actions>
        <button class="btn btn-primary" @click="openAdd">+ Add path</button>
        <button class="btn btn-ghost" @click="loadPaths">
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor"
            stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
            <polyline points="23 4 23 10 17 10"/>
            <polyline points="1 20 1 14 7 14"/>
            <path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"/>
          </svg>
          Refresh
        </button>
      </template>
    </AppHeader>

    <div class="content">
      <div v-if="loading" class="empty-state">
        <div class="spinner" />
        <span>Loading paths…</span>
      </div>

      <div v-else-if="paths.length === 0" class="empty-state">
        <svg width="56" height="56" viewBox="0 0 24 24" fill="none" stroke="currentColor"
          stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" style="opacity:.25">
          <circle cx="12" cy="12" r="10"/>
          <line x1="12" y1="8" x2="12" y2="16"/>
          <line x1="8" y1="12" x2="16" y2="12"/>
        </svg>
        <h2>No configured paths</h2>
        <p>Add a path to define a camera source or publisher endpoint.</p>
        <button class="btn btn-primary" @click="openAdd">Add first path</button>
      </div>

      <table v-else class="table">
        <thead>
          <tr>
            <th>Name</th>
            <th>Source</th>
            <th>On-demand</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="p in paths" :key="p.name">
            <td class="name-cell">{{ p.name }}</td>
            <td class="source-cell">
              <span class="badge" :class="`badge-${sourceType(p.source)}`">{{ sourceType(p.source) }}</span>
              <span v-if="p.source && p.source !== 'publisher'" class="source-url">{{ p.source }}</span>
            </td>
            <td>
              <span class="ondemand" :class="{ active: p.sourceOnDemand }">
                {{ p.sourceOnDemand ? 'Yes' : 'No' }}
              </span>
            </td>
            <td class="actions-cell">
              <button class="btn btn-ghost btn-sm" @click="openEdit(p)">Edit</button>
              <button class="btn btn-danger btn-sm" @click="askDelete(p.name)">Delete</button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <!-- Add / Edit modal -->
    <SourceModal
      v-if="modalOpen"
      :initial="editTarget"
      @close="modalOpen = false"
      @saved="loadPaths"
    />

    <!-- Delete confirmation -->
    <Teleport to="body">
      <div v-if="deleteTarget" class="modal-backdrop" @click.self="deleteTarget = null">
        <div class="confirm-dialog">
          <p>Delete path <strong>{{ deleteTarget }}</strong>?</p>
          <p class="confirm-sub">This removes it from the configuration immediately.</p>
          <div class="confirm-actions">
            <button class="btn btn-ghost" @click="deleteTarget = null">Cancel</button>
            <button class="btn btn-danger" :disabled="deleting" @click="doDelete">
              {{ deleting ? 'Deleting…' : 'Delete' }}
            </button>
          </div>
          <p v-if="deleteError" class="error-msg">{{ deleteError }}</p>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
const { listConfigPaths, deleteConfigPath } = useMediaMtx()

const paths       = ref<any[]>([])
const loading     = ref(false)
const statusText  = ref('Not connected')
const statusClass = ref('')
const modalOpen   = ref(false)
const editTarget  = ref<any>(null)
const deleteTarget = ref<string | null>(null)
const deleting    = ref(false)
const deleteError = ref('')

async function loadPaths() {
  loading.value     = true
  statusText.value  = 'Loading…'
  statusClass.value = ''
  try {
    paths.value       = await listConfigPaths()
    statusText.value  = `${paths.value.length} path${paths.value.length !== 1 ? 's' : ''}`
    statusClass.value = 'ok'
  } catch (e: any) {
    statusText.value  = 'Connection failed'
    statusClass.value = 'err'
  } finally {
    loading.value = false
  }
}

function openAdd() {
  editTarget.value = null
  modalOpen.value  = true
}

function openEdit(p: any) {
  editTarget.value = p
  modalOpen.value  = true
}

function askDelete(name: string) {
  deleteError.value  = ''
  deleteTarget.value = name
}

async function doDelete() {
  if (!deleteTarget.value) return
  deleting.value     = true
  deleteError.value  = ''
  try {
    await deleteConfigPath(deleteTarget.value)
    deleteTarget.value = null
    await loadPaths()
  } catch (e: any) {
    deleteError.value = e.message
  } finally {
    deleting.value = false
  }
}

function sourceType(source: string): string {
  if (!source || source === 'publisher') return 'publisher'
  if (source === 'redirect')             return 'redirect'
  if (source === 'rpiCamera')            return 'rpiCamera'
  if (source.startsWith('rtsp'))         return 'rtsp'
  if (source.startsWith('rtmp'))         return 'rtmp'
  if (source.startsWith('http') || source.startsWith('https')) return 'hls'
  if (source.startsWith('srt'))          return 'srt'
  if (source.startsWith('udp'))          return 'udp'
  return source.split('://')[0] ?? 'custom'
}

onMounted(loadPaths)
</script>

<style scoped>
.page-wrap { display: flex; flex-direction: column; min-height: 100dvh; }
.content { flex: 1; padding: 20px; }
.empty-state {
  display: flex; flex-direction: column;
  align-items: center; justify-content: center;
  gap: 12px; padding: 60px; text-align: center; color: var(--text-dim);
}
.empty-state h2 { font-size: 16px; font-weight: 600; color: var(--text); }
.empty-state p  { font-size: 13px; max-width: 340px; line-height: 1.6; }
.spinner {
  width: 32px; height: 32px;
  border: 2px solid var(--border);
  border-top-color: var(--accent);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}
.table { width: 100%; border-collapse: collapse; }
.table th {
  text-align: left; padding: 8px 12px;
  font-size: 11px; font-weight: 600; text-transform: uppercase;
  letter-spacing: 0.5px; color: var(--text-dim);
  border-bottom: 1px solid var(--border);
}
.table td {
  padding: 10px 12px;
  border-bottom: 1px solid var(--border);
  font-size: 13px; vertical-align: middle;
}
.name-cell { font-weight: 600; }
.source-cell { display: flex; align-items: center; gap: 8px; }
.badge {
  padding: 2px 8px; border-radius: 4px;
  font-size: 11px; font-weight: 700; text-transform: uppercase;
  letter-spacing: 0.4px; white-space: nowrap;
  background: var(--surface2); border: 1px solid var(--border); color: var(--text-dim);
}
.badge-publisher { border-color: var(--accent); color: var(--accent); background: rgba(59,130,246,.1); }
.badge-rtsp,
.badge-rtmp,
.badge-hls,
.badge-srt,
.badge-udp     { border-color: var(--green); color: var(--green); background: rgba(34,197,94,.1); }
.badge-redirect { border-color: var(--text-dim); }
.source-url {
  color: var(--text-dim); font-size: 12px;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 320px;
}
.ondemand {
  font-size: 12px; color: var(--text-dim);
  padding: 2px 8px; border-radius: 4px; border: 1px solid transparent;
}
.ondemand.active { color: var(--green); border-color: var(--green); background: rgba(34,197,94,.1); }
.actions-cell { display: flex; gap: 6px; white-space: nowrap; }
.btn-sm { height: 28px; padding: 0 10px; font-size: 12px; }
.modal-backdrop {
  position: fixed; inset: 0;
  background: rgba(0, 0, 0, 0.6);
  display: flex; align-items: center; justify-content: center; z-index: 100;
}
.confirm-dialog {
  background: var(--surface); border: 1px solid var(--border);
  border-radius: 10px; padding: 24px;
  max-width: 360px; width: 90vw;
  display: flex; flex-direction: column; gap: 12px;
  font-size: 14px;
}
.confirm-dialog strong { color: var(--text); }
.confirm-sub { font-size: 12px; color: var(--text-dim); }
.confirm-actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 4px; }
.error-msg { color: var(--red); font-size: 12px; }
</style>
