import { ref, computed } from "vue";

export type Locale = "pt" | "en";

const messages = {
  pt: {
    appName: "MetaCleaner",
    tagline: "Remove metadados localmente",
    dropzoneIdle: "Arraste um arquivo aqui",
    dropzoneClick: "ou clique para selecionar",
    cleaning: "Limpando metadados…",
    done: "Concluído",
    original: "Original",
    cleaned: "Limpo",
    open: "Abrir",
    revealInDir: "Mostrar na pasta",
    cleanAnother: "Limpar outro",
    error: "Erro",
    removedTitle: "O que foi removido",
    removedEmpty: "Nada de identificável foi encontrado neste arquivo.",
    removedCount: (n: number) => `${n} ${n === 1 ? "item removido" : "itens removidos"}`,
    strategy: {
      image: "Imagem",
      pdf: "PDF",
      "zip-office": "Office",
      svg: "SVG",
      copy: "Cópia",
    } as Record<string, string>,
    languageLabel: "Idioma",
  },
  en: {
    appName: "MetaCleaner",
    tagline: "Strip metadata locally",
    dropzoneIdle: "Drop a file here",
    dropzoneClick: "or click to choose",
    cleaning: "Cleaning metadata…",
    done: "Done",
    original: "Original",
    cleaned: "Cleaned",
    open: "Open",
    revealInDir: "Show in folder",
    cleanAnother: "Clean another",
    error: "Error",
    removedTitle: "What was removed",
    removedEmpty: "No identifying data was found in this file.",
    removedCount: (n: number) => `${n} ${n === 1 ? "item removed" : "items removed"}`,
    strategy: {
      image: "Image",
      pdf: "PDF",
      "zip-office": "Office",
      svg: "SVG",
      copy: "Copy",
    } as Record<string, string>,
    languageLabel: "Language",
  },
} as const;

const STORAGE_KEY = "metacleaner.locale";

function detectInitial(): Locale {
  try {
    const saved = localStorage.getItem(STORAGE_KEY) as Locale | null;
    if (saved === "pt" || saved === "en") return saved;
  } catch { /* ignore */ }
  const nav = (typeof navigator !== "undefined" ? navigator.language : "") || "";
  return nav.toLowerCase().startsWith("pt") ? "pt" : "en";
}

const locale = ref<Locale>(detectInitial());

export function useI18n() {
  const t = computed(() => messages[locale.value]);

  function setLocale(l: Locale) {
    locale.value = l;
    try { localStorage.setItem(STORAGE_KEY, l); } catch { /* ignore */ }
  }

  return { locale, t, setLocale };
}
