use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Serialize)]
pub struct CleanResult {
    output_path: String,
    strategy: String,
    original_size: u64,
    new_size: u64,
}

fn output_path_for(input: &Path) -> PathBuf {
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = input.extension().and_then(|s| s.to_str()).unwrap_or("");
    let parent = input.parent().unwrap_or_else(|| Path::new("."));
    let name = if ext.is_empty() {
        format!("{}_clean", stem)
    } else {
        format!("{}_clean.{}", stem, ext)
    };
    parent.join(name)
}

fn clean_image(bytes: &[u8], ext: &str) -> Result<Vec<u8>, String> {
    use img_parts::{jpeg::Jpeg, png::Png, webp::WebP, ImageEXIF, ImageICC};

    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    match ext {
        "jpg" | "jpeg" => {
            let mut img = Jpeg::from_bytes(bytes.to_vec().into())
                .map_err(|e| format!("JPEG inv\u{00e1}lido: {e}"))?;
            img.set_exif(None);
            img.set_icc_profile(None);
            img.encoder()
                .write_to(&mut out)
                .map_err(|e| format!("Falha ao escrever JPEG: {e}"))?;
        }
        "png" => {
            let img = Png::from_bytes(bytes.to_vec().into())
                .map_err(|e| format!("PNG inv\u{00e1}lido: {e}"))?;
            let mut img = img;
            img.set_exif(None);
            img.set_icc_profile(None);
            img.chunks_mut().retain(|c| {
                let t = c.kind();
                &t != b"tEXt" && &t != b"iTXt" && &t != b"zTXt" && &t != b"tIME" && &t != b"eXIf"
            });
            img.encoder()
                .write_to(&mut out)
                .map_err(|e| format!("Falha ao escrever PNG: {e}"))?;
        }
        "webp" => {
            let mut img = WebP::from_bytes(bytes.to_vec().into())
                .map_err(|e| format!("WebP inv\u{00e1}lido: {e}"))?;
            img.set_exif(None);
            img.set_icc_profile(None);
            img.chunks_mut().retain(|c| &c.id() != b"XMP ");
            img.encoder()
                .write_to(&mut out)
                .map_err(|e| format!("Falha ao escrever WebP: {e}"))?;
        }
        _ => return Err(format!("Formato de imagem n\u{00e3}o suportado: {ext}")),
    }
    Ok(out)
}

fn clean_pdf(bytes: &[u8]) -> Result<Vec<u8>, String> {
    use lopdf::{Dictionary, Document, Object};

    let mut doc = Document::load_mem(bytes).map_err(|e| format!("PDF inv\u{00e1}lido: {e}"))?;

    // 1) Substituir o objeto Info por dicion\u00e1rio vazio e remover ref\u00eancia do trailer.
    if let Ok(info_obj) = doc.trailer.get(b"Info").cloned() {
        if let Ok(info_ref) = info_obj.as_reference() {
            // Sobrescreve o objeto inteiro para garantir que /Author, /Creator, /Producer,
            // /CreationDate, /ModDate, /Title, /Subject, /Keywords sumam.
            doc.objects
                .insert(info_ref, Object::Dictionary(Dictionary::new()));
        }
    }
    doc.trailer.remove(b"Info");

    // 2) Remover XMP /Metadata do catalog.
    if let Ok(root_ref) = doc.trailer.get(b"Root").and_then(|o| o.as_reference()) {
        if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(root_ref) {
            dict.remove(b"Metadata");
        }
    }

    // 3) Remover /Metadata em qualquer p\u00e1gina.
    let page_ids: Vec<_> = doc.get_pages().values().cloned().collect();
    for page_id in page_ids {
        if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(page_id) {
            dict.remove(b"Metadata");
        }
    }

    // 4) Remover refer\u00eancias de PieceInfo (cache de aplica\u00e7\u00f5es) que carregam autoria.
    let obj_ids: Vec<_> = doc.objects.keys().cloned().collect();
    for id in obj_ids {
        if let Some(Object::Dictionary(dict)) = doc.objects.get_mut(&id) {
            dict.remove(b"PieceInfo");
            dict.remove(b"LastModified");
        }
    }

    // 5) Compactar e podar objetos n\u00e3o referenciados.
    doc.prune_objects();
    doc.compress();

    let mut out = Vec::new();
    doc.save_to(&mut out)
        .map_err(|e| format!("Falha ao salvar PDF: {e}"))?;
    Ok(out)
}

const EMPTY_CORE_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><dc:creator></dc:creator><cp:lastModifiedBy></cp:lastModifiedBy><dc:title></dc:title><dc:subject></dc:subject><dc:description></dc:description><cp:keywords></cp:keywords></cp:coreProperties>"#;

const EMPTY_APP_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"><Application></Application><Company></Company><Manager></Manager></Properties>"#;

const EMPTY_CUSTOM_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/custom-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"></Properties>"#;

const EMPTY_ODF_META: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-meta xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:meta="urn:oasis:names:tc:opendocument:xmlns:meta:1.0" office:version="1.2"><office:meta></office:meta></office:document-meta>"#;

/// Remove autoria/identidade incrustada nos XMLs internos do pacote Office:
/// - Atributos: w:author, w:lastModifiedBy, w:initials, w:userId em revisões/comentários
/// - Tags com texto: <dc:creator>, <cp:lastModifiedBy>, <Company>, <Manager>, <Application>,
///   <dc:title>, <dc:subject>, <dc:description>, <cp:keywords>, <dcterms:created/modified>
/// - ODF: <meta:initial-creator>, <meta:creation-date>, <dc:date>, <meta:editing-cycles>,
///   <meta:editing-duration>, <meta:user-defined name="Owner|Computer|...">
fn scrub_xml(input: &str) -> String {
    use once_cell::sync::Lazy;
    use regex::Regex;

    // 1) Atributos com nome/identidade. Mantém o atributo, esvazia o valor.
    static ATTRS: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r#"(?ix)
            \b(
                w:author|w:lastModifiedBy|w:initials|w:userId|
                p:authorId|c:authorId|
                meta:name|userName
            )\s*=\s*"[^"]*""#,
        )
        .unwrap()
    });
    let mut out = ATTRS.replace_all(input, "$1=\"\"").into_owned();

    // 2) Tags com conteúdo de texto. Esvazia entre abertura e fechamento.
    static TAGS: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r#"(?is)<(
                dc:creator|dc:title|dc:subject|dc:description|dc:date|
                cp:lastModifiedBy|cp:keywords|cp:lastPrinted|cp:revision|cp:category|cp:contentStatus|
                dcterms:created|dcterms:modified|
                Application|Company|Manager|AppVersion|TotalTime|
                meta:initial-creator|meta:creation-date|meta:editing-cycles|meta:editing-duration|meta:print-date|meta:printed-by|meta:generator|
                vt:lpwstr|vt:lpstr|vt:filetime
            )(\s[^>]*)?>.*?</\1>"#,
        )
        .unwrap()
    });
    out = TAGS
        .replace_all(&out, |c: &regex::Captures| {
            let tag = &c[1];
            let attrs = c.get(2).map(|m| m.as_str()).unwrap_or("");
            format!("<{tag}{attrs}></{tag}>")
        })
        .into_owned();

    // 3) Tags self-closing com data/hora (ex.: <dcterms:created xsi:type="..."/>): nada a fazer.
    out
}

fn clean_zip_office(bytes: &[u8]) -> Result<Vec<u8>, String> {
    use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

    let reader = Cursor::new(bytes);
    let mut archive = ZipArchive::new(reader).map_err(|e| format!("ZIP inv\u{00e1}lido: {e}"))?;

    let mut buf = Vec::new();
    {
        let cursor = Cursor::new(&mut buf);
        let mut writer = ZipWriter::new(cursor);

        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
            let name = file.name().to_string();
            let lower = name.to_lowercase();

            // Thumbnails carregam preview com poss\u00edveis dados sens\u00edveis: dropar.
            let drop = lower.starts_with("docprops/thumbnail")
                || lower == "thumbnails/thumbnail.png"
                || lower == "thumbnails/thumbnail.jpeg";
            if drop {
                continue;
            }

            // Substitui o conte\u00fado por uma vers\u00e3o vazia preservando o part no pacote
            // (mant\u00e9m [Content_Types].xml e _rels v\u00e1lidos).
            let replacement: Option<&str> = match lower.as_str() {
                "docprops/core.xml" => Some(EMPTY_CORE_XML),
                "docprops/app.xml" => Some(EMPTY_APP_XML),
                "docprops/custom.xml" => Some(EMPTY_CUSTOM_XML),
                "meta.xml" => Some(EMPTY_ODF_META),
                _ => None,
            };

            let options: SimpleFileOptions =
                SimpleFileOptions::default().compression_method(file.compression());

            if file.is_dir() {
                writer
                    .add_directory(&name, options)
                    .map_err(|e| e.to_string())?;
            } else if let Some(content) = replacement {
                writer
                    .start_file(&name, options)
                    .map_err(|e| e.to_string())?;
                writer
                    .write_all(content.as_bytes())
                    .map_err(|e| e.to_string())?;
            } else {
                writer
                    .start_file(&name, options)
                    .map_err(|e| e.to_string())?;
                let mut data = Vec::with_capacity(file.size() as usize);
                file.read_to_end(&mut data).map_err(|e| e.to_string())?;

                // Para arquivos .xml/.rels dentro do pacote, esfregar autoria/identidade
                // (track changes, comentários, revisões, custom props).
                let is_xmlish = lower.ends_with(".xml") || lower.ends_with(".rels");
                if is_xmlish {
                    if let Ok(text) = std::str::from_utf8(&data) {
                        let scrubbed = scrub_xml(text);
                        writer
                            .write_all(scrubbed.as_bytes())
                            .map_err(|e| e.to_string())?;
                    } else {
                        writer.write_all(&data).map_err(|e| e.to_string())?;
                    }
                } else {
                    writer.write_all(&data).map_err(|e| e.to_string())?;
                }
            }
        }

        writer.finish().map_err(|e| e.to_string())?;
    }
    Ok(buf)
}

fn detect_strategy(ext: &str) -> &'static str {
    match ext {
        "jpg" | "jpeg" | "png" | "webp" => "image",
        "pdf" => "pdf",
        "docx" | "xlsx" | "pptx" | "odt" | "ods" | "odp" | "epub" => "zip-office",
        _ => "copy",
    }
}

#[tauri::command]
fn clean_metadata(path: String) -> Result<CleanResult, String> {
    let input = PathBuf::from(&path);
    if !input.is_file() {
        return Err(format!("Arquivo n\u{00e3}o encontrado: {path}"));
    }

    let original_size = fs::metadata(&input).map(|m| m.len()).unwrap_or(0);
    let ext = input
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let bytes = fs::read(&input).map_err(|e| format!("Falha ao ler arquivo: {e}"))?;
    let strategy = detect_strategy(&ext);

    let cleaned: Vec<u8> = match strategy {
        "image" => clean_image(&bytes, &ext)?,
        "pdf" => clean_pdf(&bytes)?,
        "zip-office" => clean_zip_office(&bytes)?,
        _ => bytes.clone(),
    };

    let out_path = output_path_for(&input);
    fs::write(&out_path, &cleaned).map_err(|e| format!("Falha ao escrever sa\u{00ed}da: {e}"))?;

    Ok(CleanResult {
        output_path: out_path.to_string_lossy().to_string(),
        strategy: strategy.to_string(),
        original_size,
        new_size: cleaned.len() as u64,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![clean_metadata])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
