pub mod credentials;

use std::process::Stdio;

use serde::{Deserialize, Serialize};

use credentials::CredentialStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemResourceInfo {
    pub cpu_threads: usize,
    pub gpu_name: Option<String>,
    pub vram_total_mb: Option<u64>,
    pub vram_used_mb: Option<u64>,
    pub vram_free_mb: Option<u64>,
    pub recommended_preset: String,
    pub gpu_heavy_slots: usize,
    pub cpu_light_slots: usize,
    pub network_io_slots: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCatalogEntry {
    pub provider_id: String,
    pub name: String,
    pub version: String,
    pub capability: String,
    pub supported_languages: Vec<String>,
    pub required_hardware: String,
    pub is_local: bool,
    pub requires_credential: bool,
    pub credential_key: Option<String>,
    pub credential_configured: bool,
    pub license: String,
    pub available: bool,
    pub notes: String,
}

pub async fn probe_system_resources() -> SystemResourceInfo {
    let cpu_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    let (gpu_name, vram_total_mb, vram_used_mb, vram_free_mb) = probe_nvidia_smi().await;

    let gpu_heavy_slots = match vram_total_mb {
        Some(vram) if vram > 6144 => 2,
        _ => 1,
    };
    let recommended_preset = match vram_total_mb {
        Some(vram) if vram >= 8192 => "quality".to_string(),
        Some(vram) if vram >= 4000 => "balanced".to_string(),
        _ => "performance".to_string(),
    };

    SystemResourceInfo {
        cpu_threads,
        gpu_name,
        vram_total_mb,
        vram_used_mb,
        vram_free_mb,
        recommended_preset,
        gpu_heavy_slots,
        cpu_light_slots: (cpu_threads / 2).clamp(2, 6),
        network_io_slots: 3,
    }
}

async fn probe_nvidia_smi() -> (Option<String>, Option<u64>, Option<u64>, Option<u64>) {
    let mut cmd = tokio::process::Command::new("nvidia-smi");
    cmd.args([
        "--query-gpu=name,memory.total,memory.used,memory.free",
        "--format=csv,noheader,nounits",
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::null());

    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);

    let Ok(out) = cmd.output().await else {
        return (None, None, None, None);
    };
    if !out.status.success() {
        return (None, None, None, None);
    }

    let text = String::from_utf8_lossy(&out.stdout);
    let Some(first_line) = text.lines().next() else {
        return (None, None, None, None);
    };
    let parts: Vec<&str> = first_line.split(',').map(|s| s.trim()).collect();
    if parts.len() < 4 {
        return (None, None, None, None);
    }

    let name = Some(parts[0].to_string());
    let total = parts[1].parse::<f64>().ok().map(|v| v.round() as u64);
    let used = parts[2].parse::<f64>().ok().map(|v| v.round() as u64);
    let free = parts[3].parse::<f64>().ok().map(|v| v.round() as u64);
    (name, total, used, free)
}

pub fn list_provider_catalog() -> Vec<ProviderCatalogEntry> {
    let hf_ready = CredentialStore::has_secret("HF_TOKEN");
    let gemini_ready = CredentialStore::has_secret("GEMINI_API_KEY");
    let azure_ready = CredentialStore::has_secret("AZURE_SPEECH_KEY");

    vec![
        ProviderCatalogEntry {
            provider_id: "faster-whisper".to_string(),
            name: "Faster-Whisper (CTranslate2)".to_string(),
            version: "Stage 4".to_string(),
            capability: "stt".to_string(),
            supported_languages: vec!["zh".into(), "en".into(), "auto".into()],
            required_hardware: "GPU (4 GB VRAM int8_float16) or CPU int8".to_string(),
            is_local: true,
            requires_credential: false,
            credential_key: None,
            credential_configured: true,
            license: "MIT".to_string(),
            available: false,
            notes: "Integrated in Stage 4 with Silero VAD and word timestamps.".to_string(),
        },
        ProviderCatalogEntry {
            provider_id: "silero-vad".to_string(),
            name: "Silero VAD".to_string(),
            version: "Stage 4".to_string(),
            capability: "vad".to_string(),
            supported_languages: vec!["multilingual".into()],
            required_hardware: "CPU".to_string(),
            is_local: true,
            requires_credential: false,
            credential_key: None,
            credential_configured: true,
            license: "MIT".to_string(),
            available: false,
            notes: "Speech activity boundary detection (Stage 4).".to_string(),
        },
        ProviderCatalogEntry {
            provider_id: "pyannote-diarization".to_string(),
            name: "pyannote.audio Speaker Diarization 3.1".to_string(),
            version: "Stage 4".to_string(),
            capability: "diarization".to_string(),
            supported_languages: vec!["multilingual".into()],
            required_hardware: "GPU (4 GB VRAM sequential) or CPU".to_string(),
            is_local: true,
            requires_credential: true,
            credential_key: Some("HF_TOKEN".to_string()),
            credential_configured: hf_ready,
            license: "MIT (gated Hugging Face model terms)".to_string(),
            available: false,
            notes: "Requires Hugging Face token with pyannote/speaker-diarization-3.1 access accepted.".to_string(),
        },
        ProviderCatalogEntry {
            provider_id: "gemini-translation".to_string(),
            name: "Gemini Context-Aware Translation".to_string(),
            version: "Stage 5".to_string(),
            capability: "translation".to_string(),
            supported_languages: vec!["zh->km".into(), "zh->en".into(), "en->km".into(), "km->en".into()],
            required_hardware: "Cloud API".to_string(),
            is_local: false,
            requires_credential: true,
            credential_key: Some("GEMINI_API_KEY".to_string()),
            credential_configured: gemini_ready,
            license: "Google Gemini API Terms".to_string(),
            available: false,
            notes: "Character-aware & glossary-guided translation (Stage 5).".to_string(),
        },
        ProviderCatalogEntry {
            provider_id: "edge-neural-tts".to_string(),
            name: "Neural Khmer TTS (Piseth / Sreymom)".to_string(),
            version: "Stage 5".to_string(),
            capability: "tts".to_string(),
            supported_languages: vec!["km".into(), "en".into()],
            required_hardware: "Cloud API".to_string(),
            is_local: false,
            requires_credential: false,
            credential_key: Some("AZURE_SPEECH_KEY".to_string()),
            credential_configured: azure_ready,
            license: "Microsoft Speech Service / Edge endpoint".to_string(),
            available: false,
            notes: "Primary intelligible Khmer neural voices; optional Azure Speech key supported.".to_string(),
        },
        ProviderCatalogEntry {
            provider_id: "mms-tts-khm".to_string(),
            name: "Meta MMS-TTS Khmer (Offline Fallback)".to_string(),
            version: "Stage 5".to_string(),
            capability: "tts".to_string(),
            supported_languages: vec!["km".into()],
            required_hardware: "CPU or 4 GB GPU".to_string(),
            is_local: true,
            requires_credential: false,
            credential_key: None,
            credential_configured: true,
            license: "CC-BY-NC 4.0".to_string(),
            available: false,
            notes: "Offline single-voice Khmer fallback (review CC-BY-NC 4.0 before commercial distribution).".to_string(),
        },
    ]
}
