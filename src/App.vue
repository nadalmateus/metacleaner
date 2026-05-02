<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import { getCurrentWebview } from "@tauri-apps/api/webview";

interface CleanResult {
  output_path: string;
  strategy: string;
  original_size: number;
  new_size: number;
}

const busy = ref(false);
const error = ref("");
const result = ref<CleanResult | null>(null);
const selectedPath = ref("");
const dragOver = ref(false);

function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(2)} MB`;
}

function fileName(p: string): string {
  return p.split(/[\\/]/).pop() || p;
}

const reductionPct = computed(() => {
  if (!result.value || result.value.original_size === 0) return 0;
  const diff = result.value.original_size - result.value.new_size;
  return Math.max(0, Math.round((diff / result.value.original_size) * 100));
});

const strategyLabel: Record<string, string> = {
  image: "Imagem",
  pdf: "PDF",
  "zip-office": "Office / ODF",
  copy: "Cópia direta",
};

async function pickFile() {
  error.value = "";
  const picked = await open({
    multiple: false,
    directory: false,
    title: "Escolha um arquivo para limpar metadados",
  });
  if (typeof picked === "string") {
    await clean(picked);
  }
}

async function clean(path: string) {
  busy.value = true;
  error.value = "";
  result.value = null;
  selectedPath.value = path;
  try {
    result.value = await invoke<CleanResult>("clean_metadata", { path });
  } catch (e: any) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function openOutput() {
  if (!result.value) return;
  try { await openPath(result.value.output_path); }
  catch (e: any) { error.value = String(e); }
}

async function revealOutput() {
  if (!result.value) return;
  try { await revealItemInDir(result.value.output_path); }
  catch (e: any) { error.value = String(e); }
}

let unlistenDrag: (() => void) | null = null;
onMounted(async () => {
  try {
    unlistenDrag = await getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type === "over") {
        dragOver.value = true;
      } else if (event.payload.type === "drop") {
        dragOver.value = false;
        const paths = event.payload.paths;
        if (paths && paths.length > 0) clean(paths[0]);
      } else {
        dragOver.value = false;
      }
    });
  } catch { /* ignore */ }
});
onUnmounted(() => { if (unlistenDrag) unlistenDrag(); });
</script>

<template>
  <main class="app">
    <header>
      <div class="logo-wrap">
        <svg viewBox="0 0 64 64" class="logo" aria-hidden="true">
          <defs>
            <linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stop-color="#7c5cff" />
              <stop offset="100%" stop-color="#22d3ee" />
            </linearGradient>
          </defs>
          <rect x="14" y="12" width="38" height="48" rx="6" fill="url(#g)" />
          <path d="M22 30 h22 M22 38 h22 M22 46 h14" stroke="white" stroke-width="2.5" stroke-linecap="round" opacity=".9" />
          <circle cx="48" cy="20" r="9" fill="#0f172a" stroke="white" stroke-width="2" />
          <path d="M44 20 l3 3 l5 -6" stroke="#22d3ee" stroke-width="2.5" fill="none" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </div>
      <div>
        <h1>MetaCleaner</h1>
        <p class="subtitle">Remova metadados de qualquer arquivo, localmente.</p>
      </div>
    </header>

    <section
      class="dropzone"
      :class="{ active: dragOver, busy }"
      @click="!busy && pickFile()"
    >
      <Transition name="fade" mode="out-in">
        <div v-if="busy" key="busy" class="state">
          <div class="spinner"></div>
          <p class="title">Limpando metadados...</p>
          <p class="hint">{{ fileName(selectedPath) }}</p>
        </div>

        <div v-else-if="result" key="done" class="state">
          <div class="check">
            <svg viewBox="0 0 24 24"><path d="M5 12 l5 5 l9 -10" /></svg>
          </div>
          <p class="title">Pronto</p>
          <p class="hint">{{ fileName(result.output_path) }}</p>
        </div>

        <div v-else key="idle" class="state">
          <svg class="upload-icon" viewBox="0 0 48 48" aria-hidden="true">
            <path d="M24 32 V12 M14 22 l10 -10 l10 10" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" fill="none" />
            <path d="M8 32 V38 a4 4 0 0 0 4 4 h24 a4 4 0 0 0 4 -4 v-6" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" fill="none" />
          </svg>
          <p class="title">Arraste um arquivo aqui</p>
          <p class="hint">ou clique para selecionar</p>
        </div>
      </Transition>
    </section>

    <Transition name="slide">
      <div v-if="result" class="result-card">
        <div class="row">
          <span class="badge">{{ strategyLabel[result.strategy] || result.strategy }}</span>
          <span class="reduction" v-if="reductionPct > 0">−{{ reductionPct }}%</span>
        </div>

        <div class="sizes">
          <div class="size-block">
            <span class="size-label">Original</span>
            <span class="size-value">{{ fmtBytes(result.original_size) }}</span>
          </div>
          <svg class="arrow" viewBox="0 0 24 24"><path d="M5 12 h14 M13 6 l6 6 l-6 6" stroke="currentColor" stroke-width="2" fill="none" stroke-linecap="round" stroke-linejoin="round"/></svg>
          <div class="size-block">
            <span class="size-label">Limpo</span>
            <span class="size-value accent">{{ fmtBytes(result.new_size) }}</span>
          </div>
        </div>

        <div class="bar">
          <div class="bar-fill" :style="{ width: (100 - reductionPct) + '%' }"></div>
        </div>

        <p class="path-out" :title="result.output_path">{{ result.output_path }}</p>

        <div class="actions">
          <button class="ghost" @click="openOutput">Abrir arquivo</button>
          <button class="ghost" @click="revealOutput">Mostrar na pasta</button>
          <button class="primary" @click="pickFile">Limpar outro</button>
        </div>
      </div>
    </Transition>

    <Transition name="slide">
      <div v-if="error" class="error-card">
        <strong>Erro</strong>
        <span>{{ error }}</span>
      </div>
    </Transition>

    <footer>
      <span class="dot"></span>
      JPEG · PNG · WebP · PDF · DOCX · XLSX · PPTX · ODT · EPUB
    </footer>
  </main>
</template>

<style scoped>
.app {
  max-width: 720px;
  margin: 0 auto;
  padding: 2.5rem 1.75rem 1.5rem;
  display: flex;
  flex-direction: column;
  gap: 1.25rem;
  min-height: 100vh;
}

header {
  display: flex;
  align-items: center;
  gap: 1rem;
}
.logo {
  width: 56px;
  height: 56px;
  filter: drop-shadow(0 6px 18px rgba(124, 92, 255, 0.35));
}
h1 {
  margin: 0;
  font-size: 1.75rem;
  font-weight: 700;
  letter-spacing: -0.02em;
  background: linear-gradient(135deg, #7c5cff 0%, #22d3ee 100%);
  -webkit-background-clip: text;
  background-clip: text;
  color: transparent;
}
.subtitle {
  margin: 0.15rem 0 0;
  color: var(--muted);
  font-size: 0.92rem;
}

/* Dropzone */
.dropzone {
  position: relative;
  border-radius: 18px;
  padding: 3rem 1.5rem;
  background: var(--surface);
  border: 2px dashed var(--border);
  cursor: pointer;
  transition: all 0.25s ease;
  overflow: hidden;
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 180px;
}
.dropzone::before {
  content: "";
  position: absolute;
  inset: 0;
  background: radial-gradient(600px circle at 50% 0%, rgba(124, 92, 255, 0.12), transparent 70%);
  opacity: 0;
  transition: opacity 0.3s;
  pointer-events: none;
}
.dropzone:hover::before { opacity: 1; }
.dropzone:hover { border-color: var(--accent); }
.dropzone.active {
  border-color: var(--accent);
  background: var(--surface-hover);
  border-style: solid;
  transform: scale(1.005);
  box-shadow: 0 0 0 4px rgba(124, 92, 255, 0.2);
}
.dropzone.busy { cursor: wait; }
.state {
  text-align: center;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.55rem;
}
.upload-icon {
  width: 48px;
  height: 48px;
  color: var(--accent);
  opacity: 0.85;
}
.title { margin: 0; font-size: 1.05rem; font-weight: 600; }
.hint {
  margin: 0;
  color: var(--muted);
  font-size: 0.88rem;
  word-break: break-all;
  max-width: 520px;
}

.spinner {
  width: 44px; height: 44px;
  border: 3px solid var(--border);
  border-top-color: var(--accent);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }

.check {
  width: 52px; height: 52px;
  border-radius: 50%;
  background: linear-gradient(135deg, #10b981, #22d3ee);
  display: flex; align-items: center; justify-content: center;
  animation: pop 0.4s cubic-bezier(0.17, 0.89, 0.32, 1.49);
}
.check svg {
  width: 28px; height: 28px;
  fill: none; stroke: white; stroke-width: 3;
  stroke-linecap: round; stroke-linejoin: round;
}
@keyframes pop { from { transform: scale(0); } to { transform: scale(1); } }

/* Result card */
.result-card {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 16px;
  padding: 1.1rem 1.35rem;
  display: flex;
  flex-direction: column;
  gap: 0.85rem;
}
.row { display: flex; justify-content: space-between; align-items: center; }
.badge {
  font-size: 0.72rem;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  background: rgba(124, 92, 255, 0.15);
  color: var(--accent);
  padding: 0.25rem 0.65rem;
  border-radius: 999px;
  font-weight: 600;
}
.reduction { font-weight: 700; color: #10b981; font-size: 0.95rem; }

.sizes { display: flex; align-items: center; gap: 1rem; }
.size-block { display: flex; flex-direction: column; flex: 1; }
.size-label { font-size: 0.72rem; color: var(--muted); text-transform: uppercase; letter-spacing: 0.05em; }
.size-value { font-size: 1.1rem; font-weight: 600; }
.size-value.accent { color: var(--accent); }
.arrow { width: 20px; height: 20px; color: var(--muted); flex-shrink: 0; }

.bar { height: 6px; background: var(--border); border-radius: 999px; overflow: hidden; }
.bar-fill {
  height: 100%;
  background: linear-gradient(90deg, #7c5cff, #22d3ee);
  border-radius: 999px;
  transition: width 0.6s cubic-bezier(0.4, 0, 0.2, 1);
}

.path-out {
  margin: 0;
  font-family: ui-monospace, "SF Mono", Menlo, monospace;
  font-size: 0.76rem;
  color: var(--muted);
  background: var(--surface-hover);
  padding: 0.5rem 0.7rem;
  border-radius: 8px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.actions { display: flex; gap: 0.5rem; flex-wrap: wrap; }
button {
  font: inherit;
  padding: 0.5rem 0.95rem;
  border-radius: 9px;
  border: 1px solid transparent;
  cursor: pointer;
  font-weight: 500;
  font-size: 0.88rem;
  transition: all 0.15s;
}
button.primary { background: var(--accent); color: white; margin-left: auto; }
button.primary:hover { background: var(--accent-hover); transform: translateY(-1px); }
button.ghost { background: transparent; color: var(--text); border-color: var(--border); }
button.ghost:hover { background: var(--surface-hover); border-color: var(--accent); color: var(--accent); }

.error-card {
  background: rgba(220, 38, 38, 0.08);
  border: 1px solid rgba(220, 38, 38, 0.3);
  border-radius: 12px;
  padding: 0.85rem 1rem;
  color: #ef4444;
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
  font-size: 0.88rem;
}

footer {
  margin-top: auto;
  text-align: center;
  font-size: 0.75rem;
  color: var(--muted);
  display: flex;
  justify-content: center;
  align-items: center;
  gap: 0.5rem;
  padding-top: 0.5rem;
}
.dot {
  width: 6px; height: 6px;
  border-radius: 50%;
  background: #10b981;
  box-shadow: 0 0 8px #10b981;
}

.fade-enter-active, .fade-leave-active { transition: opacity 0.2s, transform 0.2s; }
.fade-enter-from, .fade-leave-to { opacity: 0; transform: translateY(4px); }

.slide-enter-active, .slide-leave-active { transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1); }
.slide-enter-from, .slide-leave-to { opacity: 0; transform: translateY(8px); }
</style>

<style>
:root {
  --bg: #0b0d12;
  --surface: #14171f;
  --surface-hover: #1a1e29;
  --border: #262b38;
  --text: #e6e8ef;
  --muted: #8a92a6;
  --accent: #7c5cff;
  --accent-hover: #6b4bef;

  font-family: "Inter", -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  font-size: 16px;
  color: var(--text);
  background: var(--bg);
  -webkit-font-smoothing: antialiased;
}

@media (prefers-color-scheme: light) {
  :root {
    --bg: #f7f8fb;
    --surface: #ffffff;
    --surface-hover: #f0f2f7;
    --border: #e4e7ee;
    --text: #14171f;
    --muted: #6b7280;
  }
}

html, body, #app {
  margin: 0;
  min-height: 100vh;
  background:
    radial-gradient(800px circle at 0% 0%, rgba(124, 92, 255, 0.10), transparent 50%),
    radial-gradient(600px circle at 100% 100%, rgba(34, 211, 238, 0.08), transparent 50%),
    var(--bg);
}

* { box-sizing: border-box; }
::selection { background: rgba(124, 92, 255, 0.35); }
</style>
