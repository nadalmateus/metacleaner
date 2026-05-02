<script setup lang="ts">
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

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

function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(2)} MB`;
}

async function pickFile() {
  error.value = "";
  result.value = null;
  const picked = await open({
    multiple: false,
    directory: false,
    title: "Escolha um arquivo para limpar metadados",
  });
  if (typeof picked === "string") {
    selectedPath.value = picked;
    await clean(picked);
  }
}

async function clean(path: string) {
  busy.value = true;
  error.value = "";
  result.value = null;
  try {
    result.value = await invoke<CleanResult>("clean_metadata", { path });
  } catch (e: any) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <main class="container">
    <h1>MetaCleaner</h1>
    <p class="subtitle">Remova metadados de qualquer arquivo</p>

    <div class="card">
      <button :disabled="busy" @click="pickFile">
        {{ busy ? "Processando..." : "Selecionar arquivo" }}
      </button>

      <p v-if="selectedPath" class="path">{{ selectedPath }}</p>

      <div v-if="result" class="result">
        <p><strong>Concluído</strong></p>
        <p>Estratégia: <code>{{ result.strategy }}</code></p>
        <p>Original: {{ fmtBytes(result.original_size) }} → Limpo: {{ fmtBytes(result.new_size) }}</p>
        <p class="output">Salvo em:<br /><code>{{ result.output_path }}</code></p>
      </div>

      <div v-if="error" class="error">{{ error }}</div>
    </div>

    <p class="formats">
      Suporta: JPEG, PNG, WebP, PDF, DOCX, XLSX, PPTX, ODT, EPUB.<br />
      Demais formatos são copiados sem alteração.
    </p>
  </main>
</template>

<style scoped>
.container { max-width: 640px; margin: 0 auto; padding: 4vh 2rem; text-align: center; }
h1 { margin: 0 0 .25rem; font-size: 2rem; }
.subtitle { color: #888; margin-top: 0; }
.card { border: 2px dashed #888; border-radius: 12px; padding: 2rem; margin: 2rem 0; }
button { padding: .8rem 1.6rem; font-size: 1rem; border-radius: 8px; border: 1px solid transparent; background: #396cd8; color: white; cursor: pointer; }
button:disabled { opacity: .6; cursor: not-allowed; }
button:hover:not(:disabled) { background: #2c58b8; }
.path { font-size: .85rem; color: #aaa; word-break: break-all; margin-top: 1rem; }
.result { margin-top: 1.5rem; text-align: left; }
.result code { background: rgba(128,128,128,.2); padding: .1rem .4rem; border-radius: 4px; }
.output { font-size: .85rem; word-break: break-all; }
.error { margin-top: 1rem; color: #d33; white-space: pre-wrap; }
.formats { font-size: .8rem; color: #888; }
</style>

<style>
:root {
  font-family: Inter, Avenir, Helvetica, Arial, sans-serif;
  font-size: 16px;
  color: #0f0f0f;
  background-color: #f6f6f6;
  -webkit-font-smoothing: antialiased;
}
body { margin: 0; }
@media (prefers-color-scheme: dark) {
  :root { color: #f6f6f6; background-color: #2f2f2f; }
}
</style>