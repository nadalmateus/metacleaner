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
    use lopdf::{Document, Object};

    let mut doc = Document::load_mem(bytes).map_err(|e| format!("PDF inv\u{00e1}lido: {e}"))?;

    if let Ok(info_obj) = doc.trailer.get(b"Info") {
        if let Ok(info_ref) = info_obj.as_reference() {
            if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(info_ref) {
                let keys: Vec<Vec<u8>> = dict.iter().map(|(k, _)| k.to_vec()).collect();
                for k in keys {
                    dict.remove(&k);
                }
            }
        }
    }
    doc.trailer.remove(b"Info");

    if let Ok(root_ref) = doc.trailer.get(b"Root").and_then(|o| o.as_reference()) {
        if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(root_ref) {
            dict.remove(b"Metadata");
        }
    }

    let mut out = Vec::new();
    doc.save_to(&mut out)
        .map_err(|e| format!("Falha ao salvar PDF: {e}"))?;
    Ok(out)
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
            let skip = lower == "docprops/core.xml"
                || lower == "docprops/app.xml"
                || lower == "docprops/custom.xml"
                || lower == "meta.xml"
                || lower.starts_with("docprops/thumbnail")
                || lower == "thumbnails/thumbnail.png"
                || lower == "thumbnails/thumbnail.jpeg";
            if skip {
                continue;
            }

            let options: SimpleFileOptions =
                SimpleFileOptions::default().compression_method(file.compression());

            if file.is_dir() {
                writer
                    .add_directory(&name, options)
                    .map_err(|e| e.to_string())?;
            } else {
                writer
                    .start_file(&name, options)
                    .map_err(|e| e.to_string())?;
                let mut data = Vec::with_capacity(file.size() as usize);
                file.read_to_end(&mut data).map_err(|e| e.to_string())?;
                writer.write_all(&data).map_err(|e| e.to_string())?;
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
