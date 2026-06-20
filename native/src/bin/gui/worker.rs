#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use egui::Context;
use std::sync::mpsc;
use crate::{Gui, GuiMessage, FileInfo};
use crate::tree::WebFile;
use analysis::Analysis;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(catch)]
    async fn selectFolder() -> Result<js_sys::Array, JsValue>;
}

pub fn pick_web_folder(ctx: Context, tx: mpsc::Sender<GuiMessage>) {
    wasm_bindgen_futures::spawn_local(async move {
        if let Ok(array) = selectFolder().await {
            let mut files = vec![];
            for val in array.iter() {
                let name = js_sys::Reflect::get(&val, &JsValue::from_str("name"))
                    .ok()
                    .and_then(|v| v.as_string())
                    .unwrap_or_default();
                let path = js_sys::Reflect::get(&val, &JsValue::from_str("path"))
                    .ok()
                    .and_then(|v| v.as_string())
                    .unwrap_or_default();
                if let Ok(file_val) = js_sys::Reflect::get(&val, &JsValue::from_str("file")) {
                    if let Ok(file) = file_val.dyn_into::<web_sys::File>() {
                        files.push(WebFile {
                            name,
                            path,
                            js_file: crate::tree::SendWrapper(file),
                        });
                    }
                }
            }
            // Sort files alphabetically by path
            files.sort_by(|a, b| a.path.cmp(&b.path));
            tx.send(GuiMessage::WebFolderLoaded(files)).ok();
            ctx.request_repaint();
        }
    });
}

#[wasm_bindgen]
pub fn init_worker() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub fn handle_worker_message(data: wasm_bindgen::JsValue) {
    let global = js_sys::global();
    let post_message = js_sys::Reflect::get(&global, &"postMessage".into())
        .unwrap()
        .dyn_into::<js_sys::Function>()
        .unwrap();
        
    let type_val = js_sys::Reflect::get(&data, &"type".into())
        .unwrap()
        .as_string()
        .unwrap_or_default();
        
    if type_val == "parse" {
        let path = js_sys::Reflect::get(&data, &"path".into())
            .unwrap()
            .as_string()
            .unwrap_or_default();
        let name = js_sys::Reflect::get(&data, &"name".into())
            .unwrap()
            .as_string()
            .unwrap_or_default();
        let last_modified = js_sys::Reflect::get(&data, &"lastModified".into())
            .unwrap()
            .as_f64()
            .unwrap_or(0.0);
        let size = js_sys::Reflect::get(&data, &"size".into())
            .unwrap()
            .as_f64()
            .unwrap_or(0.0);
            
        let bytes_val = js_sys::Reflect::get(&data, &"bytes".into()).unwrap();
        let uint8_array = js_sys::Uint8Array::new(&bytes_val);
        let bytes = uint8_array.to_vec();
        
        let path_clone = path.clone();
        let post_message_clone = post_message.clone();
        let start_time = web_time::SystemTime::now();
        let last_update = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        
        let progress_cb = move |processed: usize, total: usize| {
            if total > 0 {
                let elapsed_ms = start_time.elapsed().map(|d| d.as_millis() as u32).unwrap_or(0);
                let last = last_update.load(std::sync::atomic::Ordering::Relaxed);
                
                // Force update at 100% completion or throttle to ~30fps (33ms)
                if processed == total || elapsed_ms.saturating_sub(last) > 33 {
                    last_update.store(elapsed_ms, std::sync::atomic::Ordering::Relaxed);
                    let elapsed_sec = elapsed_ms as f32 / 1000.0;
                    let progress = processed as f32 / total as f32;
                    let eta_sec = if progress > 0.01 {
                        let total_estimated_sec = elapsed_sec / progress;
                        Some(total_estimated_sec - elapsed_sec)
                    } else {
                        None
                    };
                    
                    let progress_obj = js_sys::Object::new();
                    js_sys::Reflect::set(&progress_obj, &"type".into(), &"progress".into()).unwrap();
                    js_sys::Reflect::set(&progress_obj, &"path".into(), &path_clone.clone().into()).unwrap();
                    js_sys::Reflect::set(&progress_obj, &"progress".into(), &progress.into()).unwrap();
                    js_sys::Reflect::set(&progress_obj, &"elapsedSec".into(), &elapsed_sec.into()).unwrap();
                    if let Some(eta) = eta_sec {
                        js_sys::Reflect::set(&progress_obj, &"etaSec".into(), &eta.into()).unwrap();
                    }
                    
                    let _ = post_message_clone.call1(&js_sys::global(), &progress_obj);
                }
            }
        };
        
        match Analysis::try_from_bytes_with_progress(&bytes, progress_cb) {
            Ok(analysis) => {
                if let Ok(serialized) = serde_json::to_string(&analysis) {
                    let success_obj = js_sys::Object::new();
                    js_sys::Reflect::set(&success_obj, &"type".into(), &"success".into()).unwrap();
                    js_sys::Reflect::set(&success_obj, &"path".into(), &path.into()).unwrap();
                    js_sys::Reflect::set(&success_obj, &"name".into(), &name.into()).unwrap();
                    js_sys::Reflect::set(&success_obj, &"lastModified".into(), &last_modified.into()).unwrap();
                    js_sys::Reflect::set(&success_obj, &"size".into(), &size.into()).unwrap();
                    js_sys::Reflect::set(&success_obj, &"analysisJson".into(), &serialized.into()).unwrap();
                    
                    let _ = post_message.call1(&js_sys::global(), &success_obj);
                } else {
                    let error_obj = js_sys::Object::new();
                    js_sys::Reflect::set(&error_obj, &"type".into(), &"error".into()).unwrap();
                    js_sys::Reflect::set(&error_obj, &"path".into(), &path.into()).unwrap();
                    js_sys::Reflect::set(&error_obj, &"error".into(), &"Failed to serialize Analysis".into()).unwrap();
                    
                    let _ = post_message.call1(&js_sys::global(), &error_obj);
                }
            }
            Err(err) => {
                let error_obj = js_sys::Object::new();
                js_sys::Reflect::set(&error_obj, &"type".into(), &"error".into()).unwrap();
                js_sys::Reflect::set(&error_obj, &"path".into(), &path.into()).unwrap();
                js_sys::Reflect::set(&error_obj, &"error".into(), &err.into()).unwrap();
                
                let _ = post_message.call1(&js_sys::global(), &error_obj);
            }
        }
    }
}

impl Gui {
    pub fn get_or_spawn_worker(&mut self, ctx: &Context) -> Option<&web_sys::Worker> {
        if self.parser_worker.is_some() {
            return self.parser_worker.as_ref();
        }
        
        let window = web_sys::window()?;
        let document = window.document()?;
        let origin = window.location().origin().unwrap_or_default();
        
        let mut js_url = String::new();
        let mut wasm_url = String::new();
        
        // 1. Try modulepreload link or link[href*="dod-tools-gui"]
        if let Ok(Some(link)) = document.query_selector("link[rel=\"modulepreload\"]") {
            if let Ok(href) = js_sys::Reflect::get(&link, &"href".into()) {
                if let Some(href_str) = href.as_string() {
                    js_url = href_str;
                }
            }
        }
        if js_url.is_empty() {
            if let Ok(Some(link)) = document.query_selector("link[href*=\"dod-tools-gui\"][href*=\".js\"]") {
                if let Ok(href) = js_sys::Reflect::get(&link, &"href".into()) {
                    if let Some(href_str) = href.as_string() {
                        js_url = href_str;
                    }
                }
            }
        }
        
        // 2. Try link[href*=".wasm"]
        if let Ok(Some(link)) = document.query_selector("link[href*=\".wasm\"]") {
            if let Ok(href) = js_sys::Reflect::get(&link, &"href".into()) {
                if let Some(href_str) = href.as_string() {
                    wasm_url = href_str;
                }
            }
        }
        
        // 3. Fallback: scan scripts content or src
        let scripts = document.scripts();
        for i in 0..scripts.length() {
            if let Some(script) = scripts.item(i) {
                if let Ok(src) = js_sys::Reflect::get(&script, &"src".into()) {
                    if let Some(src_str) = src.as_string() {
                        if src_str.contains("dod-tools-gui") {
                            if js_url.is_empty() {
                                js_url = src_str;
                            }
                            continue;
                        }
                    }
                }
                
                if let Ok(text) = js_sys::Reflect::get(&script, &"textContent".into()) {
                    if let Some(text_str) = text.as_string() {
                        if text_str.contains("dod-tools-gui") {
                            if js_url.is_empty() {
                                if let Some(start) = text_str.find("from '") {
                                    let rest = &text_str[start + 6..];
                                    if let Some(end) = rest.find('\'') {
                                        js_url = rest[..end].to_string();
                                    }
                                } else if let Some(start) = text_str.find("from \"") {
                                    let rest = &text_str[start + 6..];
                                    if let Some(end) = rest.find('"') {
                                        js_url = rest[..end].to_string();
                                    }
                                }
                            }
                            
                            if wasm_url.is_empty() {
                                if let Some(start) = text_str.find("init('") {
                                    let rest = &text_str[start + 6..];
                                    if let Some(end) = rest.find('\'') {
                                        wasm_url = rest[..end].to_string();
                                    }
                                } else if let Some(start) = text_str.find("init(\"") {
                                    let rest = &text_str[start + 6..];
                                    if let Some(end) = rest.find('"') {
                                        wasm_url = rest[..end].to_string();
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        if js_url.is_empty() {
            js_url = "dod-tools-gui.js".to_string();
        }
        if wasm_url.is_empty() {
            wasm_url = "dod-tools-gui_bg.wasm".to_string();
        }
        
        let make_absolute = |url: String, origin: &str| -> String {
            if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("blob:") {
                url
            } else if url.starts_with('/') {
                format!("{}{}", origin, url)
            } else {
                format!("{}/{}", origin, url)
            }
        };
        
        let js_url_abs = make_absolute(js_url, &origin);
        let wasm_url_abs = make_absolute(wasm_url, &origin);
        
        let blob_code = format!(
            r#"
            self.onmessage = async function(e) {{
                const {{ type, jsUrl, wasmUrl }} = e.data;
                if (type === 'init') {{
                    try {{
                        const wasm_bindgen = await import(jsUrl);
                        await wasm_bindgen.default(wasmUrl);
                        wasm_bindgen.init_worker();
                        self.postMessage({{ type: 'ready' }});
                    }} catch (err) {{
                        self.postMessage({{ type: 'error', error: err.toString() }});
                    }}
                }}
            }};
            "#
        );
        
        let blob = web_sys::Blob::new_with_str_sequence(
            &js_sys::Array::of1(&wasm_bindgen::JsValue::from_str(&blob_code))
        ).ok()?;
        let blob_url = web_sys::Url::create_object_url_with_blob(&blob).ok()?;
        
        // Spawn the Worker as an ES module worker: new Worker(blob_url, { type: "module" })
        let options = js_sys::Object::new();
        js_sys::Reflect::set(&options, &"type".into(), &"module".into()).unwrap();
        let args = js_sys::Array::of2(&blob_url.clone().into(), &options.into());
        let global = js_sys::global();
        let worker_constructor = js_sys::Reflect::get(&global, &"Worker".into())
            .ok()
            .and_then(|v| v.dyn_into::<js_sys::Function>().ok())?;
        
        let worker: web_sys::Worker = js_sys::Reflect::construct(&worker_constructor, &args)
            .ok()
            .and_then(|w| w.dyn_into::<web_sys::Worker>().ok())?;
        
        let init_obj = js_sys::Object::new();
        js_sys::Reflect::set(&init_obj, &"type".into(), &"init".into()).unwrap();
        js_sys::Reflect::set(&init_obj, &"jsUrl".into(), &js_url_abs.into()).unwrap();
        js_sys::Reflect::set(&init_obj, &"wasmUrl".into(), &wasm_url_abs.into()).unwrap();
        let _ = worker.post_message(&init_obj);
        
        let tx = self.tx.clone();
        let ctx_for_repaint = ctx.clone();
        
        let onmessage_callback = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
            let data = event.data();
            let type_val = js_sys::Reflect::get(&data, &"type".into())
                .unwrap()
                .as_string()
                .unwrap_or_default();
                
            if type_val == "progress" {
                let path = js_sys::Reflect::get(&data, &"path".into())
                    .unwrap()
                    .as_string()
                    .unwrap_or_default();
                let progress = js_sys::Reflect::get(&data, &"progress".into())
                    .unwrap()
                    .as_f64()
                    .unwrap_or(0.0) as f32;
                let elapsed_sec = js_sys::Reflect::get(&data, &"elapsedSec".into())
                    .unwrap()
                    .as_f64()
                    .unwrap_or(0.0) as f32;
                let eta_sec = js_sys::Reflect::get(&data, &"etaSec".into())
                    .ok()
                    .and_then(|v| v.as_f64())
                    .map(|v| v as f32);
                
                let _ = tx.send(GuiMessage::DemoParsingProgress {
                    path,
                    progress,
                    elapsed_sec,
                    eta_sec,
                });
                ctx_for_repaint.request_repaint();
            } else if type_val == "success" {
                let path = js_sys::Reflect::get(&data, &"path".into())
                    .unwrap()
                    .as_string()
                    .unwrap_or_default();
                let name = js_sys::Reflect::get(&data, &"name".into())
                    .unwrap()
                    .as_string()
                    .unwrap_or_default();
                let last_modified = js_sys::Reflect::get(&data, &"lastModified".into())
                    .unwrap()
                    .as_f64()
                    .unwrap_or(0.0);
                let size = js_sys::Reflect::get(&data, &"size".into())
                    .unwrap()
                    .as_f64()
                    .unwrap_or(0.0);
                let analysis_json = js_sys::Reflect::get(&data, &"analysisJson".into())
                    .unwrap()
                    .as_string()
                    .unwrap_or_default();
                
                if let Ok(analysis) = serde_json::from_str::<Analysis>(&analysis_json) {
                    let created_at = web_time::SystemTime::UNIX_EPOCH
                        + std::time::Duration::from_millis(last_modified as u64);
                    let file_info = FileInfo {
                        created_at,
                        name,
                        path: path.clone(),
                        size_bytes: size as u64,
                    };
                    
                    let _ = tx.send(GuiMessage::WebFileParsed {
                        path,
                        file_info,
                        analysis: Box::new(analysis),
                    });
                }
                ctx_for_repaint.request_repaint();
            } else if type_val == "error" {
                let path = js_sys::Reflect::get(&data, &"path".into())
                    .unwrap()
                    .as_string()
                    .unwrap_or_default();
                let error = js_sys::Reflect::get(&data, &"error".into())
                    .unwrap()
                    .as_string()
                    .unwrap_or_default();
                
                let _ = tx.send(GuiMessage::AnalyzerError {
                    path,
                    error,
                });
                ctx_for_repaint.request_repaint();
            }
        }) as Box<dyn FnMut(web_sys::MessageEvent)>);
        
        worker.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
        onmessage_callback.forget();
        
        self.parser_worker = Some(worker);
        self.parser_worker.as_ref()
     }

     pub fn parse_web_file(&mut self, ctx: &Context, web_file: WebFile) {
        let file = &web_file.js_file.0;
        let last_modified_ms = js_sys::Reflect::get(
            file.as_ref(),
            &wasm_bindgen::JsValue::from_str("lastModified"),
        )
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
        let size_bytes = file.size() as f64;
        let path = web_file.path.clone();
        let name = web_file.name.clone();
        
        let promise = file.array_buffer();
        
        if let Some(worker) = self.get_or_spawn_worker(ctx) {
            let worker_clone = worker.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(array_buffer_val) = wasm_bindgen_futures::JsFuture::from(promise).await {
                    let parse_obj = js_sys::Object::new();
                    js_sys::Reflect::set(&parse_obj, &"type".into(), &"parse".into()).unwrap();
                    js_sys::Reflect::set(&parse_obj, &"path".into(), &path.into()).unwrap();
                    js_sys::Reflect::set(&parse_obj, &"name".into(), &name.into()).unwrap();
                    js_sys::Reflect::set(&parse_obj, &"lastModified".into(), &last_modified_ms.into()).unwrap();
                    js_sys::Reflect::set(&parse_obj, &"size".into(), &size_bytes.into()).unwrap();
                    js_sys::Reflect::set(&parse_obj, &"bytes".into(), &array_buffer_val.into()).unwrap();
                    
                    let _ = worker_clone.post_message(&parse_obj);
                }
            });
        }
     }

     pub fn parse_bytes_via_worker(&mut self, ctx: &Context, name: String, bytes: Vec<u8>) {
        if let Some(worker) = self.get_or_spawn_worker(ctx) {
            let parse_obj = js_sys::Object::new();
            js_sys::Reflect::set(&parse_obj, &"type".into(), &"parse".into()).unwrap();
            js_sys::Reflect::set(&parse_obj, &"path".into(), &name.clone().into()).unwrap();
            js_sys::Reflect::set(&parse_obj, &"name".into(), &name.clone().into()).unwrap();
            js_sys::Reflect::set(&parse_obj, &"lastModified".into(), &0.0.into()).unwrap();
            js_sys::Reflect::set(&parse_obj, &"size".into(), &(bytes.len() as f64).into()).unwrap();
            
            let uint8_array = js_sys::Uint8Array::from(bytes.as_slice());
            let array_buffer = uint8_array.buffer();
            js_sys::Reflect::set(&parse_obj, &"bytes".into(), &array_buffer.into()).unwrap();
            
            let _ = worker.post_message(&parse_obj);
        }
     }
}
