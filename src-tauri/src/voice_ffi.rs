// ABI subset generated from the pinned official sherpa-onnx v1.13.8 c-api.h.
// Upstream Apache-2.0; see resources/voice/SHERPA-LICENSE.
#![allow(non_snake_case, dead_code)]
use std::ffi::c_char;
#[repr(C)]
pub struct SherpaOnnxOfflineTtsVitsModelConfig {
    pub model: *const c_char,
    pub lexicon: *const c_char,
    pub tokens: *const c_char,
    pub data_dir: *const c_char,
    pub noise_scale: f32,
    pub noise_scale_w: f32,
    pub length_scale: f32,
    pub dict_dir: *const c_char,
}
#[repr(C)]
pub struct SherpaOnnxOfflineTtsMatchaModelConfig {
    pub acoustic_model: *const c_char,
    pub vocoder: *const c_char,
    pub lexicon: *const c_char,
    pub tokens: *const c_char,
    pub data_dir: *const c_char,
    pub noise_scale: f32,
    pub length_scale: f32,
    pub dict_dir: *const c_char,
}
#[repr(C)]
pub struct SherpaOnnxOfflineTtsKokoroModelConfig {
    pub model: *const c_char,
    pub voices: *const c_char,
    pub tokens: *const c_char,
    pub data_dir: *const c_char,
    pub length_scale: f32,
    pub dict_dir: *const c_char,
    pub lexicon: *const c_char,
    pub lang: *const c_char,
}
#[repr(C)]
pub struct SherpaOnnxOfflineTtsKittenModelConfig {
    pub model: *const c_char,
    pub voices: *const c_char,
    pub tokens: *const c_char,
    pub data_dir: *const c_char,
    pub length_scale: f32,
}
#[repr(C)]
pub struct SherpaOnnxOfflineTtsZipvoiceModelConfig {
    pub tokens: *const c_char,
    pub encoder: *const c_char,
    pub decoder: *const c_char,
    pub vocoder: *const c_char,
    pub data_dir: *const c_char,
    pub lexicon: *const c_char,
    pub feat_scale: f32,
    pub t_shift: f32,
    pub target_rms: f32,
    pub guidance_scale: f32,
}
#[repr(C)]
pub struct SherpaOnnxOfflineTtsPocketModelConfig {
    pub lm_flow: *const c_char,
    pub lm_main: *const c_char,
    pub encoder: *const c_char,
    pub decoder: *const c_char,
    pub text_conditioner: *const c_char,
    pub vocab_json: *const c_char,
    pub token_scores_json: *const c_char,
    pub voice_embedding_cache_capacity: i32,
}
#[repr(C)]
pub struct SherpaOnnxOfflineTtsSupertonicModelConfig {
    pub duration_predictor: *const c_char,
    pub text_encoder: *const c_char,
    pub vector_estimator: *const c_char,
    pub vocoder: *const c_char,
    pub tts_json: *const c_char,
    pub unicode_indexer: *const c_char,
    pub voice_style: *const c_char,
}
#[repr(C)]
pub struct SherpaOnnxOfflineTtsModelConfig {
    pub vits: SherpaOnnxOfflineTtsVitsModelConfig,
    pub num_threads: i32,
    pub debug: i32,
    pub provider: *const c_char,
    pub matcha: SherpaOnnxOfflineTtsMatchaModelConfig,
    pub kokoro: SherpaOnnxOfflineTtsKokoroModelConfig,
    pub kitten: SherpaOnnxOfflineTtsKittenModelConfig,
    pub zipvoice: SherpaOnnxOfflineTtsZipvoiceModelConfig,
    pub pocket: SherpaOnnxOfflineTtsPocketModelConfig,
    pub supertonic: SherpaOnnxOfflineTtsSupertonicModelConfig,
}
#[repr(C)]
pub struct SherpaOnnxOfflineTtsConfig {
    pub model: SherpaOnnxOfflineTtsModelConfig,
    pub rule_fsts: *const c_char,
    pub max_num_sentences: i32,
    pub rule_fars: *const c_char,
    pub silence_scale: f32,
}
#[repr(C)]
pub struct SherpaOnnxGeneratedAudio {
    pub samples: *const f32,
    pub n: i32,
    pub sample_rate: i32,
}
