<template>
    <div class="page-wrap">
        <AppHeader
            :status-text="statusText"
            :status-class="statusClass"
            @connect="connect"
        >
            <template #actions>
                <button v-if="connected" class="btn btn-ghost" @click="connect">
                    <svg
                        width="14"
                        height="14"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2.5"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                    >
                        <polyline points="23 4 23 10 17 10" />
                        <polyline points="1 20 1 14 7 14" />
                        <path
                            d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"
                        />
                    </svg>
                    Refresh
                </button>
                <div v-if="connected" class="layout-picker">
                    <button
                        v-for="c in [1, 2, 3]"
                        :key="c"
                        class="layout-btn"
                        :class="{ active: cols === c }"
                        @click="setLayout(c)"
                    >
                        {{ c === 1 ? "1" : c === 2 ? "4" : "9" }}
                    </button>
                </div>
            </template>
        </AppHeader>

        <!-- Stream selector bar (shown when more streams than fit on one page) -->
        <div v-if="allStreams.length > PAGE_SIZE" class="stream-bar">
            <span class="bar-label">Streams:</span>
            <span
                v-for="name in allStreams"
                :key="name"
                class="stream-tag"
                :class="{ active: selectedNames.includes(name) }"
                @click="toggleStream(name)"
                >{{ name }}</span
            >
        </div>

        <!-- Empty / error state -->
        <div v-if="pageSlice.length === 0" class="empty-state">
            <svg
                width="64"
                height="64"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.2"
                stroke-linecap="round"
                stroke-linejoin="round"
            >
                <rect x="2" y="7" width="20" height="15" rx="2" />
                <polyline points="17 2 12 7 7 2" />
            </svg>
            <h2>{{ emptyTitle }}</h2>
            <p>{{ emptyMsg }}</p>
        </div>

        <!-- Camera grid — key on renderKey so cells fully remount on connect/layout change -->
        <main
            v-else
            :key="renderKey"
            class="grid"
            :style="{ gridTemplateColumns: `repeat(${cols}, 1fr)` }"
        >
            <CameraCell v-for="name in pageSlice" :key="name" :name="name" />
        </main>

        <!-- Pagination -->
        <div v-if="pageCount > 1" class="pagination">
            <button class="page-btn" @click="prevPage">‹</button>
            <button
                v-for="i in pageCount"
                :key="i"
                class="page-btn"
                :class="{ active: page === i - 1 }"
                @click="page = i - 1"
            >
                {{ i }}
            </button>
            <button class="page-btn" @click="nextPage">›</button>
        </div>
    </div>
</template>

<script setup lang="ts">
const { listPaths } = useMediaMtx();

const PAGE_SIZE = 9;

const allStreams = ref<string[]>([]);
const selectedNames = ref<string[]>([]);
const cols = ref(3);
const page = ref(0);
const renderKey = ref(0);
const connected = ref(false);
const statusText = ref("Not connected");
const statusClass = ref("");
const emptyTitle = ref("No streams loaded");
const emptyMsg = ref(
    "Enter your MediaMTX server address and click Connect to discover available streams.",
);

const perPage = computed(() => cols.value * cols.value);
const displayNames = computed(() =>
    selectedNames.value.length ? selectedNames.value : allStreams.value,
);
const pageCount = computed(() =>
    Math.ceil(displayNames.value.length / perPage.value),
);
const pageSlice = computed(() => {
    const start = page.value * perPage.value;
    return displayNames.value.slice(start, start + perPage.value);
});

async function connect() {
    statusText.value = "Connecting…";
    statusClass.value = "";
    allStreams.value = [];
    selectedNames.value = [];
    page.value = 0;
    renderKey.value++;

    try {
        allStreams.value = await listPaths();

        if (allStreams.value.length === 0) {
            statusText.value = "No active streams";
            statusClass.value = "err";
            emptyTitle.value = "No active streams";
            emptyMsg.value = "Server reachable but no active streams found.";
            connected.value = true;
            return;
        }

        selectedNames.value = [...allStreams.value];
        statusText.value = `${allStreams.value.length} stream${allStreams.value.length !== 1 ? "s" : ""}`;
        statusClass.value = "ok";
        connected.value = true;
        renderKey.value++;
    } catch (e: any) {
        statusText.value = "Connection failed";
        statusClass.value = "err";
        emptyTitle.value = "Connection failed";
        emptyMsg.value = `Could not reach MediaMTX API: ${e.message}`;
    }
}

function setLayout(c: number) {
    cols.value = c;
    page.value = 0;
    renderKey.value++;
}

function toggleStream(name: string) {
    const idx = selectedNames.value.indexOf(name);
    if (idx === -1) {
        if (selectedNames.value.length >= PAGE_SIZE)
            selectedNames.value.shift();
        selectedNames.value.push(name);
    } else {
        selectedNames.value.splice(idx, 1);
    }
    page.value = 0;
    renderKey.value++;
}

function prevPage() {
    if (page.value > 0) {
        page.value--;
        renderKey.value++;
    }
}
function nextPage() {
    if (page.value < pageCount.value - 1) {
        page.value++;
        renderKey.value++;
    }
}

onMounted(connect);
</script>

<style scoped>
.page-wrap {
    display: flex;
    flex-direction: column;
    min-height: 100dvh;
}
.stream-bar {
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    padding: 8px 20px;
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    align-items: center;
}
.bar-label {
    font-size: 12px;
    color: var(--text-dim);
    margin-right: 4px;
}
.stream-tag {
    padding: 4px 10px;
    border-radius: 20px;
    border: 1px solid var(--border);
    background: var(--surface2);
    color: var(--text-dim);
    font-size: 12px;
    cursor: pointer;
    transition: all 0.15s;
    user-select: none;
}
.stream-tag.active {
    border-color: var(--accent);
    background: rgba(59, 130, 246, 0.15);
    color: var(--text);
}
.stream-tag:hover {
    border-color: var(--accent);
}
.layout-picker {
    display: flex;
    gap: 4px;
}
.layout-btn {
    width: 32px;
    height: 32px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--surface2);
    color: var(--text-dim);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 11px;
    font-weight: 700;
    transition: all 0.15s;
}
.layout-btn.active {
    background: rgba(59, 130, 246, 0.2);
    border-color: var(--accent);
    color: var(--text);
}
.layout-btn:hover:not(.active) {
    border-color: var(--accent);
}
.empty-state {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 16px;
    color: var(--text-dim);
    text-align: center;
    padding: 40px;
}
.empty-state svg {
    opacity: 0.25;
}
.empty-state h2 {
    font-size: 16px;
    font-weight: 600;
    color: var(--text);
}
.empty-state p {
    font-size: 13px;
    max-width: 360px;
    line-height: 1.6;
}
.grid {
    flex: 1;
    padding: 16px;
    display: grid;
    gap: 10px;
}
.pagination {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 10px 0 16px;
}
.page-btn {
    width: 32px;
    height: 32px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--surface2);
    color: var(--text);
    cursor: pointer;
    font-size: 13px;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.15s;
}
.page-btn.active {
    background: var(--accent);
    border-color: var(--accent);
    font-weight: 700;
}
.page-btn:hover:not(.active) {
    border-color: var(--accent);
}
</style>
