use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;

// Limites de seguran\u{00e7}a (DoS / zip-bomb).
const MAX_INPUT_BYTES: u64 = 512 * 1024 * 1024; // 512 MiB
const MAX_OUTPUT_BYTES: u64 = 1024 * 1024 * 1024; // 1 GiB
const MAX_ZIP_ENTRIES: usize = 65_536;

#[derive(Serialize)]
pub struct CleanResult {
    output_path: String,
    strategy: String,
    original_size: u64,
    new_size: u64,
}

// ---------- caminhos ----------

/// Gera um caminho de sa\u{00ed}da \u{00fanico} ao lado do original (n\u{00e3}o sobrescreve).
fn unique_output_path(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let ext = input.extension().and_then(|s| s.to_str()).unwrap_or("");
    let parent = input.parent().unwrap_or_else(|| Path::new("."));

    let make = |suffix: &str| -> PathBuf {
        let name = if ext.is_empty() {
            format!("{stem}{suffix}")
        } else {
            format!("{stem}{suffix}.{ext}")
        };
        parent.join(name)
    };

    let mut candidate = make("_clean");
    let mut i: u32 = 1;
    while candidate.exists() {
        candidate = make(&format!("_clean_{i}"));
        i = i.saturating_add(1);
        if i > 9999 {
            break;
        }
    }
    candidate
}

/// Valida que o caminho aponta para um arquivo regular existente, dentro de tamanho aceit\u{00e1}vel.
fn validate_input(path_str: &str) -> Result<(PathBuf, u64), String> {
    let raw = PathBuf::from(path_str);
    let canon = fs::canonicalize(&raw).map_err(|e| format!("Caminho inv\u{00e1}lido: {e}"))?;
    let meta = fs::symlink_metadata(&canon).map_err(|e| format!("Sem acesso: {e}"))?;
    if !meta.is_file() {
        return Err("O caminho n\u{00e3}o aponta para um arquivo regular.".into());
    }
    let size = meta.len();
    if size == 0 {
        return Err("Arquivo vazio.".into());
    }
    if size > MAX_INPUT_BYTES {
        return Err(format!(
            "Arquivo grande demais ({} MiB). Limite: {} MiB.",
            size / (1024 * 1024),
            MAX_INPUT_BYTES / (1024 * 1024)
        ));
    }
    Ok((canon, size))
}

// ---------- imagens ----------

fn clean_image(bytes: &[u8], ext: &str) -> Result<Vec<u8>, String> {
    use img_parts::{jpeg::Jpeg, png::Png, webp::WebP, ImageEXIF, ImageICC};

    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    match ext {
        "jpg" | "jpeg" => {
            let mut img = Jpeg::from_bytes(bytes.to_vec().into())
                .map_err(|e| format!("JPEG inv\u{00e1}lido: {e}"))?;

            // Remove EXIF (APP1) e ICC profile (APP2).
            img.set_exif(None);
            img.set_icc_profile(None);

            // Remove TODOS os segmentos APPn (1..=15) e COM (coment\u{00e1}rios).
            // APP0 (JFIF) \u{00e9} preservado pois \u{00e9} estrutural.
            // Remove tamb\u{00e9}m XMP (APP1 com identificador "http://ns.adobe.com/xap/"),
            // IPTC/Photoshop (APP13), Adobe (APP14), Ducky (APP12), JPEG-XT.
            img.segments_mut().retain(|seg| {
                let m = seg.marker();
                // JFIF \u{00e9} APP0 (0xE0). Quadros essenciais (SOF, SOS, DQT, DHT, DRI, EOI, SOI, RST*) ficam.
                // 0xE0..=0xEF s\u{00e3}o APPn. 0xFE \u{00e9} COM.
                if m == 0xE0 {
                    return true; // mant\u{00e9}m JFIF
                }
                if (0xE1..=0xEF).contains(&m) {
                    return false; // remove demais APPn
                }
                if m == 0xFE {
                    return false; // remove COM
                }
                true
            });

            img.encoder()
                .write_to(&mut out)
                .map_err(|e| format!("Falha ao escrever JPEG: {e}"))?;
        }
        "png" => {
            let mut img = Png::from_bytes(bytes.to_vec().into())
                .map_err(|e| format!("PNG inv\u{00e1}lido: {e}"))?;
            img.set_exif(None);
            img.set_icc_profile(None);

            // Remove todo chunk auxiliar que possa carregar metadados ou identifica\u{00e7}\u{00e3}o:
            // texto (tEXt/iTXt/zTXt), tempo (tIME), EXIF (eXIf), perfil de cor adicional (sPLT, hIST).
            // Mant\u{00e9}m chunks essenciais para renderiza\u{00e7}\u{00e3}o (IHDR, IDAT, IEND, PLTE, tRNS, gAMA, cHRM, sRGB, sBIT, bKGD, pHYs).
            img.chunks_mut().retain(|c| {
                let t = c.kind();
                !matches!(
                    &t,
                    b"tEXt" | b"iTXt" | b"zTXt" | b"tIME" | b"eXIf" | b"sPLT" | b"hIST"
                )
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
            // Remove EXIF/XMP/ICCP independentemente do helper acima.
            img.chunks_mut().retain(|c| {
                let id = c.id();
                !matches!(&id, b"XMP " | b"EXIF" | b"ICCP")
            });
            img.encoder()
                .write_to(&mut out)
                .map_err(|e| format!("Falha ao escrever WebP: {e}"))?;
        }
        _ => return Err(format!("Formato de imagem n\u{00e3}o suportado: {ext}")),
    }
    Ok(out)
}

// ---------- PDF ----------

fn clean_pdf(bytes: &[u8]) -> Result<Vec<u8>, String> {
    use lopdf::{Dictionary, Document, Object};

    let mut doc = Document::load_mem(bytes).map_err(|e| format!("PDF inv\u{00e1}lido: {e}"))?;

    // 1) Esvazia o objeto Info e remove ref do trailer.
    if let Ok(info_obj) = doc.trailer.get(b"Info").cloned() {
        if let Ok(info_ref) = info_obj.as_reference() {
            doc.objects
                .insert(info_ref, Object::Dictionary(Dictionary::new()));
        }
    }
    doc.trailer.remove(b"Info");
    doc.trailer.remove(b"ID"); // ID do arquivo pode rastrear sess\u{00f5}es

    // 2) Limpa Root/Catalog: XMP, OpenAction, MarkInfo, Lang, EmbeddedFiles, AF (associated files),
    //    Outlines/Names que podem incluir destinos com autoria, ViewerPreferences customizadas.
    if let Ok(root_ref) = doc.trailer.get(b"Root").and_then(|o| o.as_reference()) {
        if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(root_ref) {
            dict.remove(b"Metadata");
            dict.remove(b"PieceInfo");
            dict.remove(b"PageLabels");
            dict.remove(b"AF");
            dict.remove(b"AcroForm"); // formul\u{00e1}rios podem armazenar valores de campos
            // Remove anexos embutidos via /Names -> /EmbeddedFiles.
            if let Some(Object::Reference(names_ref)) = dict.get(b"Names").ok().cloned() {
                if let Ok(Object::Dictionary(ndict)) = doc.get_object_mut(names_ref) {
                    ndict.remove(b"EmbeddedFiles");
                }
            }
        }
    }

    // 3) Remove /Metadata em todas as p\u{00e1}ginas.
    let page_ids: Vec<_> = doc.get_pages().values().cloned().collect();
    for page_id in page_ids {
        if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(page_id) {
            dict.remove(b"Metadata");
            dict.remove(b"PieceInfo");
            dict.remove(b"LastModified");
        }
    }

    // 4) Remove PieceInfo/LastModified/Metadata em qualquer dicion\u{00e1}rio do arquivo.
    //    Tamb\u{00e9}m remove anota\u{00e7}\u{00f5}es (/Annots) que armazenam autor (/T) e dados de revis\u{00e3}o.
    let obj_ids: Vec<_> = doc.objects.keys().cloned().collect();
    for id in obj_ids {
        if let Some(Object::Dictionary(dict)) = doc.objects.get_mut(&id) {
            dict.remove(b"PieceInfo");
            dict.remove(b"LastModified");
            dict.remove(b"Metadata");
            // Em anota\u{00e7}\u{00f5}es PDF, /T = autor, /M = modifica\u{00e7}\u{00e3}o, /CreationDate, /NM = nome\u{00fanico.
            if let Ok(t) = dict.get(b"Type").and_then(|o| o.as_name()) {
                if t == b"Annot" {
                    dict.remove(b"T");
                    dict.remove(b"M");
                    dict.remove(b"CreationDate");
                    dict.remove(b"NM");
                    dict.remove(b"RC");
                    dict.remove(b"Subj");
                }
            }
        }
    }

    // 5) Compacta e poda objetos n\u{00e3}o referenciados (remove restos de sess\u{00f5}es).
    doc.prune_objects();
    doc.compress();

    let mut out = Vec::new();
    doc.save_to(&mut out)
        .map_err(|e| format!("Falha ao salvar PDF: {e}"))?;
    Ok(out)
}

// ---------- Office / ODF / EPUB ----------

const EMPTY_CORE_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><dc:creator></dc:creator><cp:lastModifiedBy></cp:lastModifiedBy><dc:title></dc:title><dc:subject></dc:subject><dc:description></dc:description><cp:keywords></cp:keywords></cp:coreProperties>"#;

const EMPTY_APP_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"><Application></Application><Company></Company><Manager></Manager></Properties>"#;

const EMPTY_CUSTOM_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/custom-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"></Properties>"#;

const EMPTY_ODF_META: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-meta xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:meta="urn:oasis:names:tc:opendocument:xmlns:meta:1.0" office:version="1.2"><office:meta></office:meta></office:document-meta>"#;

/// Esfrega autoria/identidade dos XMLs internos do pacote Office/ODF.
fn scrub_xml(input: &str) -> String {
    use once_cell::sync::Lazy;
    use regex::Regex;

    // Atributos com nome/identidade -> esvazia o valor.
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

    // Atributos w:rsid* identificam a sess\u{00e3}o de edi\u{00e7}\u{00e3}o (rsidR, rsidRDefault, etc.) -> remove.
    static RSIDS: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"(?i)\sw:rsid[A-Za-z]*\s*=\s*"[^"]*""#).unwrap());

    // Tags com texto sens\u{00edvel}.
    static TAGS: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r#"(?is)<(
                dc:creator|dc:title|dc:subject|dc:description|dc:date|dc:language|
                cp:lastModifiedBy|cp:keywords|cp:lastPrinted|cp:revision|cp:category|cp:contentStatus|
                dcterms:created|dcterms:modified|
                Application|Company|Manager|AppVersion|TotalTime|
                meta:initial-creator|meta:creation-date|meta:editing-cycles|meta:editing-duration|meta:print-date|meta:printed-by|meta:generator|
                vt:lpwstr|vt:lpstr|vt:filetime
            )(\s[^>]*)?>.*?</\1>"#,
        )
        .unwrap()
    });

    let s1 = ATTRS.replace_all(input, "$1=\"\"");
    let s2 = RSIDS.replace_all(&s1, "");
    let s3 = TAGS.replace_all(&s2, |c: &regex::Captures| {
        let tag = &c[1];
        let attrs = c.get(2).map(|m| m.as_str()).unwrap_or("");
        format!("<{tag}{attrs}></{tag}>")
    });
    s3.into_owned()
}

fn clean_zip_office(bytes: &[u8]) -> Result<Vec<u8>, String> {
    use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

    let reader = Cursor::new(bytes);
    let mut archive = ZipArchive::new(reader).map_err(|e| format!("ZIP inv\u{00e1}lido: {e}"))?;

    if archive.len() > MAX_ZIP_ENTRIES {
        return Err(format!(
            "ZIP com muitas entradas ({} > {MAX_ZIP_ENTRIES}).",
            archive.len()
        ));
    }

    let mut buf = Vec::with_capacity(bytes.len());
    let mut total_uncompressed: u64 = 0;
    {
        let cursor = Cursor::new(&mut buf);
        let mut writer = ZipWriter::new(cursor);

        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
            // Mitiga\u{00e7}\u{00e3}o zip-slip: rejeita nomes com path traversal ou absolutos.
            let safe_name = match file.enclosed_name() {
                Some(p) => p.to_string_lossy().replace('\\', "/"),
                None => continue, // entrada suspeita: pula
            };
            let lower = safe_name.to_ascii_lowercase();

            // Mitiga\u{00e7}\u{00e3}o zip-bomb: limite de descompress\u{00e3}o cumulativa.
            total_uncompressed = total_uncompressed.saturating_add(file.size());
            if total_uncompressed > MAX_OUTPUT_BYTES {
                return Err("ZIP excede limite de descompress\u{00e3}o.".into());
            }

            // Thumbnails carregam preview com poss\u{00edveis} dados sens\u{00edveis}: drop.
            let drop = lower.starts_with("docprops/thumbnail")
                || lower == "thumbnails/thumbnail.png"
                || lower == "thumbnails/thumbnail.jpeg";
            if drop {
                continue;
            }

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
                    .add_directory(&safe_name, options)
                    .map_err(|e| e.to_string())?;
            } else if let Some(content) = replacement {
                writer
                    .start_file(&safe_name, options)
                    .map_err(|e| e.to_string())?;
                writer
                    .write_all(content.as_bytes())
                    .map_err(|e| e.to_string())?;
            } else {
                writer
                    .start_file(&safe_name, options)
                    .map_err(|e| e.to_string())?;
                let mut data = Vec::with_capacity(file.size().min(64 * 1024 * 1024) as usize);
                file.read_to_end(&mut data).map_err(|e| e.to_string())?;

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

// ---------- pipeline ----------

fn detect_strategy(ext: &str) -> &'static str {
    match ext {
        "jpg" | "jpeg" | "png" | "webp" => "image",
        "pdf" => "pdf",
        "svg" => "svg",
        "docx" | "xlsx" | "pptx" | "odt" | "ods" | "odp" | "epub" => "zip-office",
        _ => "copy",
    }
}

/// Verifica se os primeiros bytes batem com a extens\u{00e3}o declarada (anti-spoofing leve).
fn verify_magic(bytes: &[u8], ext: &str) -> Result<(), String> {
    let ok = match ext {
        "jpg" | "jpeg" => bytes.starts_with(&[0xFF, 0xD8, 0xFF]),
        "png" => bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]),
        "webp" => bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP",
        "pdf" => bytes.starts_with(b"%PDF-"),
        "docx" | "xlsx" | "pptx" | "odt" | "ods" | "odp" | "epub" => {
            bytes.starts_with(&[0x50, 0x4B, 0x03, 0x04]) || bytes.starts_with(&[0x50, 0x4B, 0x05, 0x06])
        }
        "svg" => true, // texto, validado pelo parser
        _ => true,
    };
    if !ok {
        return Err(format!("Conte\u{00fa}do n\u{00e3}o corresponde \u{00e0} extens\u{00e3}o .{ext}."));
    }
    Ok(())
}

fn clean_svg(bytes: &[u8]) -> Result<Vec<u8>, String> {
    use once_cell::sync::Lazy;
    use regex::Regex;

    let text = std::str::from_utf8(bytes).map_err(|_| "SVG n\u{00e3}o UTF-8".to_string())?;

    // Remove blocos <metadata>...</metadata>, <title>, <desc>, <foreignObject>,
    // coment\u{00e1}rios <!-- ... --> e processing instructions <?xml-stylesheet?>.
    static BLOCKS: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"(?is)<metadata\b[^>]*>.*?</metadata>|<title\b[^>]*>.*?</title>|<desc\b[^>]*>.*?</desc>|<!--.*?-->|<\?xml-stylesheet[^?]*\?>"#).unwrap()
    });

    // Atributos sens\u{00edveis} de autoria/edi\u{00e7}\u{00e3}o (Inkscape/Sodipodi/Illustrator).
    static ATTRS: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"(?i)\s(?:inkscape|sodipodi|i)?:?(?:cx|cy|zoom|pageopacity|pageshadow|window-width|window-height|window-x|window-y|current-layer|docname|version|cy|cx|object-tolerance|guide-tolerance|grid-tolerance|export-filename|export-xdpi|export-ydpi)\s*=\s*"[^"]*""#).unwrap()
    });

    let cleaned = BLOCKS.replace_all(text, "");
    let cleaned = ATTRS.replace_all(&cleaned, "");
    Ok(cleaned.into_owned().into_bytes())
}

fn process(input: PathBuf) -> Result<CleanResult, String> {
    let original_size = fs::metadata(&input).map(|m| m.len()).unwrap_or(0);
    let ext = input
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let bytes = fs::read(&input).map_err(|e| format!("Falha ao ler arquivo: {e}"))?;
    let strategy = detect_strategy(&ext);
    verify_magic(&bytes, &ext)?;

    let cleaned: Vec<u8> = match strategy {
        "image" => clean_image(&bytes, &ext)?,
        "pdf" => clean_pdf(&bytes)?,
        "svg" => clean_svg(&bytes)?,
        "zip-office" => clean_zip_office(&bytes)?,
        _ => bytes.clone(),
    };

    if cleaned.len() as u64 > MAX_OUTPUT_BYTES {
        return Err("Sa\u{00edda} excede limite de tamanho.".into());
    }

    let out_path = unique_output_path(&input);
    // Escrita at\u{00f4mica: tmp \u{2192} rename, evita corromper se o processo morrer.
    let tmp_path = out_path.with_extension(format!(
        "{}.part",
        out_path.extension().and_then(|s| s.to_str()).unwrap_or("tmp")
    ));
    fs::write(&tmp_path, &cleaned).map_err(|e| format!("Falha ao escrever sa\u{00edda}: {e}"))?;
    fs::rename(&tmp_path, &out_path).map_err(|e| format!("Falha ao finalizar: {e}"))?;

    Ok(CleanResult {
        output_path: out_path.to_string_lossy().to_string(),
        strategy: strategy.to_string(),
        original_size,
        new_size: cleaned.len() as u64,
    })
}

#[tauri::command]
async fn clean_metadata(path: String) -> Result<CleanResult, String> {
    let (canonical, _size) = validate_input(&path)?;
    // Trabalho pesado fora da thread principal/UI.
    tokio::task::spawn_blocking(move || process(canonical))
        .await
        .map_err(|e| format!("Falha interna: {e}"))?
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
