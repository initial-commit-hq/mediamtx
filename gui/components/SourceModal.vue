<template>
    <Teleport to="body">
        <div class="modal-backdrop" @click.self="$emit('close')">
            <div class="modal">
                <div class="modal-header">
                    <h2>{{ isEdit ? "Edit Path" : "Add Path" }}</h2>
                    <button class="btn-icon" @click="$emit('close')">✕</button>
                </div>

                <div class="modal-body">
                    <div class="field">
                        <label>Path name</label>
                        <input
                            v-model="form.name"
                            :disabled="isEdit"
                            placeholder="my-camera"
                        />
                    </div>

<!-- <div class="field">-->
<!--     <label>Source type</label>-->
<!--     <select v-model="form.sourceType">-->
<!--         <option value="rtsp">RTSP</option>-->
<!--         <option value="rtmp">RTMP</option>-->
<!--         <option value="hls">HLS</option>-->
<!--         <option value="srt">SRT</option>-->
<!--         <option value="redirect">Redirect</option>-->
<!--     </select>-->
<!-- </div>-->

                    <div class="field">
                        <label>Source</label>
                        <input
                            v-model="form.source"
                            :placeholder="sourcePlaceholder"
                        />
                    </div>

                    <div class="field">
                        <label>RTSP transport</label>
                        <select v-model="form.rtspTransport">
                            <option value="">automatic (MediaMTX default)</option>
                            <option value="tcp">TCP</option>
                            <option value="udp">UDP</option>
                            <option value="multicast">multicast</option>
                        </select>
                        <p v-if="rtsptHint" class="field-hint">
                            This source uses <code>rtspt://</code>, which is not a real URI
                            scheme &mdash; MediaMTX rejects it. Use <code>rtsp://</code> with
                            transport <strong>TCP</strong>; the scheme is rewritten on save.
                        </p>
                    </div>

                    <div class="field field-row">
                        <label>On-demand</label>
                        <button
                            class="toggle"
                            :class="{ on: form.sourceOnDemand }"
                            type="button"
                            @click="form.sourceOnDemand = !form.sourceOnDemand"
                        >
                            {{ form.sourceOnDemand ? "Enabled" : "Disabled" }}
                        </button>
                    </div>

                    <p v-if="error" class="error-msg">{{ error }}</p>
                </div>

                <div class="modal-footer">
                    <button class="btn btn-ghost" @click="$emit('close')">
                        Cancel
                    </button>
                    <button
                        class="btn btn-primary"
                        :disabled="saving"
                        @click="save"
                    >
                        {{
                            saving
                                ? "Saving…"
                                : isEdit
                                  ? "Save changes"
                                  : "Add path"
                        }}
                    </button>
                </div>
            </div>
        </div>
    </Teleport>
</template>

<script setup lang="ts">
const props = defineProps<{
    initial?: any;
}>();
const emit = defineEmits(["close", "saved"]);

const { addConfigPath, patchConfigPath } = useMediaMtx();

const isEdit = computed(() => !!props.initial);

function inferSourceType(source: string): string {
    if (!source || source.startsWith("rtsp")) return "rtsp";
    if (source.startsWith("rtmp")) return "rtmp";
    if (source.startsWith("http")) return "hls";
    if (source.startsWith("srt")) return "srt";
    if (source === "redirect") return "redirect";
    return "rtsp";
}

const form = reactive({
    name: props.initial?.name ?? "",
    // sourceType: inferSourceType(props.initial?.source ?? ""),
    source:
        props.initial?.source && props.initial.source !== "rtsp"
            ? props.initial.source
            : "",
    sourceOnDemand: props.initial?.sourceOnDemand ?? false,
    // "" means leave it unset so MediaMTX keeps its own default. An explicit
    // rtspTransport is what makes an rtspt:// camera work: rtspt is a
    // Live555/FFmpeg convention for "RTSP forced over TCP", not a URI scheme, and
    // MediaMTX rejects it with `invalid source`. It expresses the same intent as
    // rtsp:// plus this option.
    rtspTransport: props.initial?.rtspTransport ?? "",
});

const error = ref("");
const saving = ref(false);

// Shown while the field still holds a pseudo-scheme, so the reason the transport
// matters is visible at the point of entry rather than after a 400.
const rtsptHint = computed(() => /^rtsp[tu]:\/\//i.test(form.source ?? ""));

const sourcePlaceholder = computed(() => {
    switch (inferSourceType(form.source ?? "")) {
        case "rtsp":
            return "rtsp://user:pass@127.0.0.1:554/stream";
        case "rtmp":
            return "rtmp://127.0.0.1/live/stream";
        case "hls":
            return "http://127.0.0.1:8888/stream/index.m3u8";
        case "srt":
            return "srt://127.0.0.1:8890?streamid=stream";
        case "redirect":
            return "rtsp://other-server/stream";
        default:
            return "";
    }
});

async function save() {
    error.value = "";
    if (!form.name.trim()) {
        error.value = "Path name is required";
        return;
    }
    if (!form.source.trim()) {
        error.value = "Source URL is required";
        return;
    }

    saving.value = true;

    // Normalise the pseudo-schemes rather than letting MediaMTX reject them.
    // Cameras are commonly configured as rtspt:// (force TCP) or rtspu:// (force
    // UDP); neither is a registered scheme, and MediaMTX answers
    //   400 invalid source: 'rtspt://...'
    // Rewriting to rtsp:// and setting the matching transport preserves intent.
    let source = form.source.trim();
    let rtspTransport = form.rtspTransport;
    const m = source.match(/^rtsp([tu]):\/\//i);
    if (m) {
        source = source.replace(/^rtsp[tu]:\/\//i, "rtsp://");
        if (!rtspTransport) {
            rtspTransport = m[1].toLowerCase() === "t" ? "tcp" : "udp";
        }
    }

    const body: Record<string, any> = {
        name: form.name.trim(),
        source,
        sourceOnDemand: form.sourceOnDemand,
    };
    // Omitted when empty so MediaMTX keeps its own default instead of being
    // handed an empty string.
    if (rtspTransport) {
        body.rtspTransport = rtspTransport;
    }

    try {
        if (isEdit.value) {
            await patchConfigPath(form.name, body);
        } else {
            await addConfigPath(form.name.trim(), body);
        }
        emit("saved");
        emit("close");
    } catch (e: any) {
        error.value = e.message;
    } finally {
        saving.value = false;
    }
}
</script>

<style scoped>
.modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
}
.modal {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    width: 560px;
    max-width: 95vw;
    overflow: hidden;
}
.modal-header {
    padding: 16px 20px;
    border-bottom: 1px solid var(--border);
    display: flex;
    align-items: center;
    justify-content: space-between;
}
.modal-header h2 {
    font-size: 15px;
    font-weight: 600;
}
.btn-icon {
    background: none;
    border: none;
    color: var(--text-dim);
    cursor: pointer;
    font-size: 16px;
    padding: 4px;
    line-height: 1;
}
.btn-icon:hover {
    color: var(--text);
}
.modal-body {
    padding: 20px;
    display: flex;
    flex-direction: column;
    gap: 14px;
}
.field {
    display: flex;
    flex-direction: column;
    gap: 6px;
}
.field-hint {
    margin: 6px 0 0;
    font-size: 12px;
    line-height: 1.45;
    color: var(--text-muted, #9aa0a6);
}
.field-hint code {
    padding: 1px 4px;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.08);
}
.field label {
    font-size: 12px;
    color: var(--text-dim);
}
.field input,
.field select {
    background: var(--surface2);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text);
    padding: 8px 12px;
    font-size: 13px;
    outline: none;
    width: 100%;
}
.field input:focus,
.field select:focus {
    border-color: var(--accent);
}
.field input:disabled {
    opacity: 0.5;
    cursor: not-allowed;
}
.field select option {
    background: var(--surface2);
}
.field-row {
    flex-direction: row;
    align-items: center;
    justify-content: space-between;
}
.toggle {
    padding: 5px 14px;
    border-radius: 20px;
    border: 1px solid var(--border);
    background: var(--surface2);
    color: var(--text-dim);
    cursor: pointer;
    font-size: 12px;
    font-weight: 600;
    transition: all 0.15s;
}
.toggle.on {
    background: rgba(59, 130, 246, 0.2);
    border-color: var(--accent);
    color: var(--text);
}
.error-msg {
    color: var(--red);
    font-size: 12px;
}
.modal-footer {
    padding: 16px 20px;
    border-top: 1px solid var(--border);
    display: flex;
    justify-content: flex-end;
    gap: 8px;
}
</style>
