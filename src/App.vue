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
  "zip-office": "Office",
  copy: "Cópia",
};

async function pickFile() {
  error.value = "";
  const picked = await open({ multiple: false, directory: false });
  if (typeof picked === "string") await clean(picked);
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
      if (event.payload.type === "over") dragOver.value = true;
      else if (event.payload.type === "drop") {
        dragOver.value = false;
        const paths = event.payload.paths;
        if (paths && paths.length > 0) clean(paths[0]);
      } else dragOver.value = false;
    });
  } catch { /* ignore */ }
});
onUnmounted(() => { if (unlistenDrag) unlistenDrag(); });
</script>

<template>
  <main class="mx-auto flex min-h-screen max-w-xl flex-col gap-6 px-6 py-10">
    <!-- Header -->
    <header class="flex items-center gap-3">
      <div class="flex h-9 w-9 items-center justify-center rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)]">
        <svg viewBox="0 0 24 24" class="h-4.5 w-4.5 text-[var(--color-accent)]" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M3 6h18M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2M5 6l1 14a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2l1-14"/>
        </svg>
      </div>
      <div class="leading-tight">
        <h1 class="text-base font-semibold tracking-tight">MetaCleaner</h1>
        <p class="text-xs text-[var(--color-muted)]">Remove metadados localmente</p>
      </div>
    </header>

    <!-- Dropzone -->
    <section
      @click="!busy && pickFile()"
      class="group relative flex min-h-[180px] cursor-pointer items-center justify-center rounded-xl border border-dashed border-[var(--color-border)] bg-[var(--color-surface)] px-6 py-10 transition-all hover:border-[var(--color-accent)] hover:bg-[var(--color-surface-2)]"
      :class="{
        '!border-solid !border-[var(--color-accent)] !bg-[var(--color-surface-2)] ring-2 ring-[var(--color-accent)]/30': dragOver,
        'cursor-wait': busy
      }"
    >
      <Transition name="fade" mode="out-in">
        <div v-if="busy" key="busy" class="flex flex-col items-center gap-3 text-center">
          <div class="h-7 w-7 animate-spin rounded-full border-2 border-[var(--color-border)] border-t-[var(--color-accent)]"></div>
          <p class="text-sm font-medium">Limpando metadados…</p>
          <p class="max-w-[28rem] truncate text-xs text-[var(--color-muted)]">{{ fileName(selectedPath) }}</p>
        </div>

        <div v-else-if="result" key="done" class="flex flex-col items-center gap-3 text-center">
          <div class="flex h-10 w-10 items-center justify-center rounded-full bg-[var(--color-success)]/15 text-[var(--color-success)]">
            <svg viewBox="0 0 24 24" class="h-5 w-5" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
              <path d="M5 12l5 5 9-10"/>
            </svg>
          </div>
          <p class="text-sm font-medium">Concluído</p>
          <p class="max-w-[28rem] truncate text-xs text-[var(--color-muted)]">{{ fileName(result.output_path) }}</p>
        </div>

        <div v-else key="idle" class="flex flex-col items-center gap-3 text-center">
          <svg viewBox="0 0 24 24" class="h-7 w-7 text-[var(--color-muted)] transition-colors group-hover:text-[var(--color-accent)]" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">
            <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
            <polyline points="17 8 12 3 7 8"/>
            <line x1="12" y1="3" x2="12" y2="15"/>
          </svg>
          <p class="text-sm font-medium">Arraste um arquivo aqui</p>
          <p class="text-xs text-[var(--color-muted)]">ou clique para selecionar</p>
        </div>
      </Transition>
    </section>

    <!-- Result -->
    <Transition name="slide">
      <div v-if="result" class="flex flex-col gap-4 rounded-xl border border-[var(--color-border)] bg-[var(--color-surface)] p-4">
        <div class="flex items-center justify-between">
          <span class="rounded-md border border-[var(--color-border)] bg-[var(--color-surface-2)] px-2 py-0.5 text-[11px] font-medium uppercase tracking-wider text-[var(--color-muted)]">
            {{ strategyLabel[result.strategy] || result.strategy }}
          </span>
          <span v-if="reductionPct > 0" class="text-xs font-semibold text-[var(--color-success)]">
            −{{ reductionPct }}%
          </span>
        </div>

        <div class="grid grid-cols-2 gap-3 text-sm">
          <div>
            <div class="text-[11px] uppercase tracking-wider text-[var(--color-muted)]">Original</div>
            <div class="mt-0.5 font-medium tabular-nums">{{ fmtBytes(result.original_size) }}</div>
          </div>
          <div>
            <div class="text-[11px] uppercase tracking-wider text-[var(--color-muted)]">Limpo</div>
            <div class="mt-0.5 font-medium tabular-nums text-[var(--color-accent)]">{{ fmtBytes(result.new_size) }}</div>
          </div>
        </div>

        <div class="h-1 w-full overflow-hidden rounded-full bg-[var(--color-surface-2)]">
          <div class="h-full bg-[var(--color-accent)] transition-[width] duration-500" :style="{ width: (100 - reductionPct) + '%' }"></div>
        </div>

        <div class="rounded-md border border-[var(--color-border)] bg-[var(--color-surface-2)] px-2.5 py-1.5 font-mono text-[11px] text-[var(--color-muted)]" :title="result.output_path">
          <div class="truncate">{{ result.output_path }}</div>
        </div>

        <div class="flex flex-wrap gap-2">
          <button @click="openOutput" class="btn-ghost">Abrir</button>
          <button @click="revealOutput" class="btn-ghost">Mostrar na pasta</button>
          <button @click="pickFile" class="btn-primary ml-auto">Limpar outro</button>
        </div>
      </div>
    </Transition>

    <!-- Error -->
    <Transition name="slide">
      <div v-if="error" class="flex flex-col gap-1 rounded-xl border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-400">
        <strong class="text-xs uppercase tracking-wider">Erro</strong>
        <span class="text-[13px]">{{ error }}</span>
      </div>
    </Transition>

    <!-- Footer -->
    <footer class="mt-auto flex items-center justify-center gap-2 pt-4 text-[11px] text-[var(--color-muted)]">
      <span class="h-1.5 w-1.5 rounded-full bg-[var(--color-success)]"></span>
      JPEG · PNG · WebP · PDF · DOCX · XLSX · PPTX · ODT · EPUB
    </footer>
  </main>
</template>

<style>
@reference "./style.css";

.btn-primary {
  @apply rounded-lg bg-[var(--color-accent)] px-3 py-1.5 text-sm font-medium text-white transition-colors;
}
.btn-primary:hover { background: var(--color-accent-hover); }

.btn-ghost {
  @apply rounded-lg border border-[var(--color-border)] bg-transparent px-3 py-1.5 text-sm font-medium text-[var(--color-text)] transition-colors;
}
.btn-ghost:hover {
  border-color: var(--color-accent);
  color: var(--color-accent);
}

.fade-enter-active, .fade-leave-active { transition: opacity .18s, transform .18s; }
.fade-enter-from, .fade-leave-to { opacity: 0; transform: translateY(3px); }

.slide-enter-active, .slide-leave-active { transition: all .25s cubic-bezier(.4,0,.2,1); }
.slide-enter-from, .slide-leave-to { opacity: 0; transform: translateY(6px); }
</style>
