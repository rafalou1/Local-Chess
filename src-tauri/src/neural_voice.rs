//! Persistent, CPU-only native Kokoro via the pinned official sherpa-onnx C ABI.
//! All foreign handles and buffers stay on a single worker. No service or Python.
use crate::voice_ffi::*;
use base64::Engine;
use libloading::Library;
use serde::Serialize;
use std::{
    collections::VecDeque,
    ffi::{c_void, CStr, CString},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc, OnceLock,
    },
};
use tauri::{Emitter, Manager};
static GENERATION: AtomicU64 = AtomicU64::new(0);
static WORKER: OnceLock<mpsc::Sender<Request>> = OnceLock::new();
#[derive(Clone, Serialize)]
pub struct Audio {
    pub stream_chunks: usize,
    pub provider: String,
    pub wav_base64: String,
    pub sample_rate: i32,
    pub duration_ms: f64,
    pub generation_ms: f64,
    pub cache_hit: bool,
    pub voice: String,
}
struct Request {
    chunks: AtomicUsize,
    app: tauri::AppHandle,
    stream: bool,
    root: PathBuf,
    text: String,
    voice: i32,
    speed: f32,
    sequence: u64,
    reply: mpsc::Sender<Result<Option<Audio>, String>>,
}
type Create = unsafe extern "C" fn(*const SherpaOnnxOfflineTtsConfig) -> *const c_void;
type Destroy = unsafe extern "C" fn(*const c_void);
type Generate = unsafe extern "C" fn(
    *const c_void,
    *const std::ffi::c_char,
    i32,
    f32,
    unsafe extern "C" fn(*const f32, i32, *mut c_void) -> i32,
    *mut c_void,
) -> *const SherpaOnnxGeneratedAudio;
type FreeAudio = unsafe extern "C" fn(*const SherpaOnnxGeneratedAudio);
struct Native {
    _library: Library,
    _ort: Library,
    tts: *const c_void,
    generate: Generate,
    destroy: Destroy,
    free_audio: FreeAudio,
}
impl Drop for Native {
    fn drop(&mut self) {
        unsafe { (self.destroy)(self.tts) }
    }
}
fn encode_wav(pcm: &[f32], sr: i32) -> String {
    let mut wav = Vec::with_capacity(44 + pcm.len() * 2);
    let bytes = (pcm.len() * 2) as u32;
    wav.extend(b"RIFF");
    wav.extend((bytes + 36).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend((sr as u32).to_le_bytes());
    wav.extend((sr as u32 * 2).to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend(bytes.to_le_bytes());
    for v in pcm {
        wav.extend(((v.clamp(-1., 1.) * 32767.) as i16).to_le_bytes())
    }
    base64::engine::general_purpose::STANDARD.encode(wav)
}
unsafe extern "C" fn continue_generation(samples: *const f32, n: i32, arg: *mut c_void) -> i32 {
    let r = &*(arg as *const Request);
    if GENERATION.load(Ordering::Acquire) != r.sequence {
        return 0;
    }
    if r.stream && n > 0 && n < 24000 * 120 && !samples.is_null() {
        let wav = encode_wav(
            std::slice::from_raw_parts(samples, n as usize),
            if r.voice >= 100 { 22050 } else { 24000 },
        );
        let index = r.chunks.fetch_add(1, Ordering::AcqRel);
        let _ = r.app.emit(
            "neural-audio",
            serde_json::json!({"sequence":r.sequence,"index":index,"wav_base64":wav}),
        );
    }
    1
}

impl Native {
    unsafe fn new(root: &std::path::Path, voice: i32) -> Result<Self, String> {
        let load = |p: PathBuf| -> Result<Library, String> {
            libloading::os::windows::Library::load_with_flags(p, 0x100 | 0x1000)
                .map(Library::from)
                .map_err(|e| e.to_string())
        };
        let ort = load(root.join("onnxruntime.dll"))?;
        let library = load(root.join("sherpa-onnx-c-api.dll"))?;
        let version: *const std::ffi::c_char = library
            .get::<unsafe extern "C" fn() -> *const std::ffi::c_char>(b"SherpaOnnxGetVersionStr\0")
            .map_err(|e| e.to_string())?();
        if !CStr::from_ptr(version).to_string_lossy().contains("1.13.8") {
            return Err("Unexpected neural voice runtime version".into());
        }
        let create = *library
            .get::<Create>(b"SherpaOnnxCreateOfflineTts\0")
            .map_err(|e| e.to_string())?;
        let generate = *library
            .get::<Generate>(b"SherpaOnnxOfflineTtsGenerateWithCallbackWithArg\0")
            .map_err(|e| e.to_string())?;
        let destroy = *library
            .get::<Destroy>(b"SherpaOnnxDestroyOfflineTts\0")
            .map_err(|e| e.to_string())?;
        let free_audio = *library
            .get::<FreeAudio>(b"SherpaOnnxDestroyOfflineTtsGeneratedAudio\0")
            .map_err(|e| e.to_string())?;
        // Tauri may return an extended Windows path. The engine's C stdio/espeak
        // readers require an ordinary drive path, unlike Rust's filesystem APIs.
        let path = |s: &str| {
            CString::new(
                root.join(s)
                    .to_string_lossy()
                    .trim_start_matches(r"\\?\")
                    .as_bytes(),
            )
            .map_err(|e| e.to_string())
        };
        let model = path("model.int8.onnx")?;
        let voices = path("voices.bin")?;
        let tokens = path("tokens.txt")?;
        let data = path("espeak-ng-data")?;
        let lexicon = path("lexicon-us-en.txt")?;
        let cpu = CString::new("cpu").unwrap();
        let lang = CString::new("en-us").unwrap();
        let mut config: SherpaOnnxOfflineTtsConfig = std::mem::zeroed();
        config.model.kokoro.model = model.as_ptr();
        config.model.kokoro.voices = voices.as_ptr();
        config.model.kokoro.tokens = tokens.as_ptr();
        config.model.kokoro.data_dir = data.as_ptr();
        config.model.kokoro.lexicon = lexicon.as_ptr();
        config.model.kokoro.lang = lang.as_ptr();
        config.model.kokoro.length_scale = 1.;
        config.model.num_threads = 2;
        config.model.provider = cpu.as_ptr();
        config.max_num_sentences = 1;
        config.silence_scale = 0.75;
        let fast_model = path(&format!("piper/{}/model.onnx", voice))?;
        let fast_tokens = path(&format!("piper/{}/tokens.txt", voice))?;
        if voice >= 100 {
            config.model.kokoro = std::mem::zeroed();
            config.model.vits.model = fast_model.as_ptr();
            config.model.vits.tokens = fast_tokens.as_ptr();
            config.model.vits.data_dir = data.as_ptr();
            config.model.vits.noise_scale = 0.667;
            config.model.vits.noise_scale_w = 0.8;
            config.model.vits.length_scale = 1.;
        }
        let tts = create(&config);
        if tts.is_null() {
            return Err(format!(
                "Neural voice could not initialize from {}",
                root.display()
            ));
        }
        Ok(Self {
            _library: library,
            _ort: ort,
            tts,
            generate,
            destroy,
            free_audio,
        })
    }
    unsafe fn generate(&self, r: &Request) -> Result<Audio, String> {
        let started = std::time::Instant::now();
        let text = CString::new(r.text.as_str()).map_err(|e| e.to_string())?;
        let audio = (self.generate)(
            self.tts,
            text.as_ptr(),
            if r.voice >= 100 { 0 } else { r.voice },
            r.speed,
            continue_generation,
            (r as *const Request).cast_mut().cast(),
        );
        if audio.is_null() {
            return Err("Neural voice returned no audio".into());
        }
        let a = &*audio;
        if a.n <= 0 || a.n > 24000 * 120 || a.sample_rate <= 0 {
            (self.free_audio)(audio);
            return Err("Invalid neural audio length".into());
        }
        let pcm = std::slice::from_raw_parts(a.samples, a.n as usize);
        let sr = a.sample_rate;
        let wav_base64 = encode_wav(pcm, sr);
        let duration_ms = a.n as f64 / sr as f64 * 1000.;
        (self.free_audio)(audio);
        if GENERATION.load(Ordering::Acquire) != r.sequence {
            return Err("Speech cancelled".into());
        }
        Ok(Audio {
            stream_chunks: r.chunks.load(Ordering::Acquire),
            provider: if r.voice >= 100 {
                "Piper native offline"
            } else {
                "Kokoro native offline"
            }
            .into(),
            wav_base64,
            sample_rate: sr,
            duration_ms,
            generation_ms: started.elapsed().as_secs_f64() * 1000.,
            cache_hit: false,
            voice: match r.voice {
                100 => "Ryan",
                101 => "Lessac",
                102 => "Thorsten",
                26 => "George",
                24 => "Daniel",
                3 => "Heart",
                _ => "Michael",
            }
            .into(),
        })
    }
}
fn sender() -> &'static mpsc::Sender<Request> {
    WORKER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Request>();
        std::thread::spawn(move || {
            // Keep the current and previous voice resident. Language/voice
            // switches otherwise reload ONNX and add seconds to the next reply.
            // Low-memory systems retain the original single-model bound.
            let model_limit = if sysinfo::System::new_all().available_memory() < 3 * 1024 * 1024 * 1024 { 1 } else { 2 };
            let mut models: VecDeque<(i32, Native, bool)> = VecDeque::new();
            let mut cache: VecDeque<(String, Audio)> = VecDeque::new();
            for r in rx {
                let result = (|| {
                    if GENERATION.load(Ordering::Acquire) != r.sequence {
                        return if r.text.is_empty() { Ok(None) } else { Err("Speech cancelled".into()) };
                    }
                    let key = format!("{}:{:.2}:{}", r.voice, r.speed, r.text);
                    // Cached audio needs no model load at all.
                    if !r.text.is_empty() {
                        if let Some((_, a)) = cache.iter().find(|(k, _)| *k == key) {
                            let mut a = a.clone();
                            a.cache_hit = true;
                            a.generation_ms = 0.;
                            a.stream_chunks = 0;
                            return Ok(Some(a));
                        }
                    }
                    let model = if r.voice >= 100 { r.voice } else { 0 };
                    if let Some(index) = models.iter().position(|n| n.0 == model) {
                        let entry = models.remove(index).unwrap();
                        models.push_back(entry);
                    } else {
                        while models.len() >= model_limit { models.pop_front(); }
                        models.push_back((model, unsafe { Native::new(&r.root, r.voice)? }, false));
                    }
                    let native = models.back_mut().unwrap();
                    if r.text.is_empty() {
                        if native.2 { return Ok(None); }
                        let warm = Request {
                            chunks: AtomicUsize::new(0),
                            app: r.app.clone(),
                            stream: false,
                            root: r.root.clone(),
                            text: if r.voice == 102 { "Bereit." } else { "Ready." }.into(),
                            voice: r.voice,
                            speed: r.speed,
                            sequence: r.sequence,
                            reply: r.reply.clone(),
                        };
                        native.2 = unsafe { native.1.generate(&warm) }.is_ok();
                        return Ok(None);
                    }
                    let a = unsafe { native.1.generate(&r)? };
                    native.2 = true;
                    cache.push_back((key, a.clone()));
                    if cache.len() > 12 {
                        cache.pop_front();
                    }
                    Ok(Some(a))
                })();
                let _ = r.reply.send(result);
            }
        });
        tx
    })
}
fn root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let p = app
        .path()
        .resource_dir()
        .map_err(|e| e.to_string())?
        .join("resources/voice");
    Ok(if p.exists() {
        p
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/voice")
    })
}
#[tauri::command]
pub async fn neural_speech(
    app: tauri::AppHandle,
    text: String,
    voice: i32,
    speed: f32,
    sequence: Option<u64>,
    stream: Option<bool>,
) -> Result<Option<Audio>, String> {
    if text.chars().count() > 600 {
        return Err("Coach speech is limited to 600 characters".into());
    }
    if ![16, 26, 24, 3, 100, 101, 102].contains(&voice) {
        return Err("Unknown neural voice".into());
    }
    let root = root(&app)?;
    let sequence = if let Some(seq) = sequence {
        GENERATION.fetch_max(seq, Ordering::AcqRel);
        seq
    } else {
        GENERATION.fetch_add(1, Ordering::AcqRel) + 1
    };
    tauri::async_runtime::spawn_blocking(move || {
        let (tx, rx) = mpsc::channel();
        sender()
            .send(Request {
                chunks: AtomicUsize::new(0),
                app,
                stream: stream.unwrap_or(false),
                root,
                text,
                voice,
                speed: speed.clamp(0.8, 1.2),
                sequence,
                reply: tx,
            })
            .map_err(|_| "Voice worker unavailable")?;
        rx.recv_timeout(std::time::Duration::from_secs(30))
            .map_err(|_| "Voice generation timed out")?
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn cancel_neural(sequence: Option<u64>) {
    if let Some(seq) = sequence {
        GENERATION.fetch_max(seq, Ordering::AcqRel);
    } else {
        GENERATION.fetch_add(1, Ordering::AcqRel);
    }
}
