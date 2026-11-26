use std::path::Path;
use std::process::Stdio;

use crate::media::MediaEngineStatus;

async fn run_command_capture(bin: &str, args: &[&str]) -> Result<String, String> {
    let mut cmd = tokio::process::Command::new(bin);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let out = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to invoke {bin}: {e}"))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("{bin} failed ({}): {}", out.status, stderr.trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub async fn probe_media_engine() -> MediaEngineStatus {
    let ffmpeg_ver = run_command_capture("ffmpeg", &["-version"]).await;
    let ffprobe_ver = run_command_capture("ffprobe", &["-version"]).await;

    let ffmpeg_version = ffmpeg_ver
        .as_ref()
        .ok()
        .and_then(|s| s.lines().next())
        .map(|l| l.trim().to_string());

    let encoders = run_command_capture("ffmpeg", &["-hide_banner", "-encoders"])
        .await
        .unwrap_or_default();
    let filters = run_command_capture("ffmpeg", &["-hide_banner", "-filters"])
        .await
        .unwrap_or_default();

    let error = match (&ffmpeg_ver, &ffprobe_ver) {
        (Err(e), _) => Some(e.clone()),
        (_, Err(e)) => Some(e.clone()),
        _ => None,
    };

    MediaEngineStatus {
        ffmpeg_available: ffmpeg_ver.is_ok(),
        ffprobe_available: ffprobe_ver.is_ok(),
        ffmpeg_version,
        has_nvenc: encoders.contains("h264_nvenc"),
        has_libx264: encoders.contains("libx264"),
        has_libass: filters.contains(" ass ") || filters.contains(" subtitles "),
        has_rubberband: filters.contains(" rubberband "),
        error,
    }
}

pub async fn extract_audio_wav(
    input_media: &Path,
    output_wav: &Path,
    sample_rate: u32,
    channels: u8,
) -> Result<(), String> {
    if let Some(parent) = output_wav.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create audio output directory: {e}"))?;
    }

    let mut cmd = tokio::process::Command::new("ffmpeg");
    cmd.arg("-v")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(input_media)
        .arg("-vn")
        .arg("-ac")
        .arg(channels.to_string())
        .arg("-ar")
        .arg(sample_rate.to_string())
        .arg("-c:a")
        .arg("pcm_s16le")
        .arg(output_wav)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let out = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to run ffmpeg audio extraction: {e}"))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!(
            "ffmpeg audio extraction failed ({}): {}",
            out.status,
            stderr.trim()
        ));
    }

    Ok(())
}

/// Compute normalized amplitude peaks (0.0..=1.0) from a 16-bit PCM WAV file.
pub fn compute_wav_waveform_peaks(wav_path: &Path, bucket_count: usize) -> Result<Vec<f32>, String> {
    let bytes = std::fs::read(wav_path)
        .map_err(|e| format!("Failed to read WAV for waveform ({}): {e}", wav_path.display()))?;

    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("Invalid RIFF/WAVE header".to_string());
    }

    let mut pos = 12usize;
    let mut data_slice: Option<&[u8]> = None;
    while pos + 8 <= bytes.len() {
        let chunk_id = &bytes[pos..pos + 4];
        let chunk_size = u32::from_le_bytes([
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]) as usize;
        let data_start = pos + 8;
        let data_end = data_start.saturating_add(chunk_size).min(bytes.len());
        if chunk_id == b"data" {
            data_slice = Some(&bytes[data_start..data_end]);
            break;
        }
        // RIFF chunks are padded to even byte boundaries.
        pos = data_start + chunk_size + (chunk_size % 2);
    }

    let pcm = data_slice.ok_or_else(|| "WAV file has no data chunk".to_string())?;
    let total_samples = pcm.len() / 2;
    if total_samples == 0 || bucket_count == 0 {
        return Ok(vec![0.0; bucket_count]);
    }

    let buckets = bucket_count.min(total_samples).max(1);
    let samples_per_bucket = (total_samples as f64 / buckets as f64).max(1.0);
    let mut peaks = Vec::with_capacity(buckets);

    for b in 0..buckets {
        let start_idx = ((b as f64) * samples_per_bucket).floor() as usize;
        let end_idx = (((b + 1) as f64) * samples_per_bucket)
            .ceil()
            .min(total_samples as f64) as usize;

        let mut max_amp: f32 = 0.0;
        for s in start_idx..end_idx.max(start_idx + 1).min(total_samples) {
            let offset = s * 2;
            let sample = i16::from_le_bytes([pcm[offset], pcm[offset + 1]]);
            let amp = (sample as i32).abs() as f32 / 32768.0;
            if amp > max_amp {
                max_amp = amp;
            }
        }
        peaks.push((max_amp * 1000.0).round() / 1000.0);
    }

    Ok(peaks)
}
