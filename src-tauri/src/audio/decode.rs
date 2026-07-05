//! Pure-Rust audio normalization to the canonical 16 kHz / mono / signed-16-bit
//! PCM WAV consumed by Whisper, Parakeet, and the cloud transcription path.
//!
//! Mirrors ffmpeg -ac 1 -ar 16000 -sample_fmt s16 without spawning a process.
//! Phase 1 handles every container/codec Symphonia supports EXCEPT Opus
//! (Phase 2 adds libopus). This module is additive: it does NOT touch the
//! existing ffmpeg converter and is only exercised by its own tests.

#![allow(dead_code)]

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use symphonia::core::audio::{AudioBufferRef, Channels};
use symphonia::core::codecs::{CODEC_TYPE_NULL, CODEC_TYPE_OPUS};
use symphonia::core::errors::Error;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::audio::resampler::resample_to_16khz;

const TARGET_RATE: u32 = 16_000;
const TARGET_CHANNELS: u16 = 1;
const TARGET_BITS: u16 = 16;

/// Decode any supported audio/video-container file to the canonical Whisper /
/// Parakeet / cloud WAV: 16 kHz, mono, signed-16-bit PCM.
///
/// Non-streaming in this phase (decodes fully, then resamples); Phase 3 makes
/// it streaming. Opus is NOT handled yet and returns Err("opus not yet
/// supported").
///
/// This function NEVER panics to the caller: any panic raised by Symphonia,
/// rubato, or hound on a corrupt or unsupported file is caught and converted to
/// Err("audio decode failed (unsupported or corrupt file)").
pub fn normalize_to_wav(input: &Path, output: &Path) -> Result<(), String> {
    let input = input.to_path_buf();
    let output = output.to_path_buf();
    match catch_unwind(AssertUnwindSafe(move || normalize_inner(&input, &output))) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("audio decode failed (unsupported or corrupt file)".to_string()),
    }
}

fn normalize_inner(input: &Path, output: &Path) -> Result<(), String> {
    // 1. WAV passthrough: already canonical (16 kHz / mono / 16-bit int)?
    if try_passthrough_canonical_wav(input, output)? {
        return Ok(());
    }

    // 2. Decode + downmix to mono f32 at the source sample rate.
    let (mono, src_rate) = decode_to_mono(input)?;

    // 3. Resample to 16 kHz via the existing rubato-based resampler.
    let resampled = if src_rate == TARGET_RATE {
        mono
    } else {
        resample_to_16khz(&mono, src_rate)?
    };

    // 4. Quantize to s16 and write the WAV atomically (temp + rename).
    write_s16_wav(&resampled, output)?;
    Ok(())
}

/// If input is a WAV that is already 16 kHz / mono / 16-bit int, byte-copy it
/// to output and return Ok(true). Returns Ok(false) otherwise (including when
/// the file is not a WAV or hound cannot parse it).
fn try_passthrough_canonical_wav(input: &Path, output: &Path) -> Result<bool, String> {
    let is_wav = input
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.eq_ignore_ascii_case("wav"))
        .unwrap_or(false);
    if !is_wav {
        return Ok(false);
    }
    let Ok(reader) = WavReader::open(input) else {
        return Ok(false);
    };
    let spec = reader.spec();
    if spec.sample_rate == TARGET_RATE
        && spec.channels == TARGET_CHANNELS
        && spec.bits_per_sample == TARGET_BITS
        && spec.sample_format == SampleFormat::Int
    {
        std::fs::copy(input, output)
            .map_err(|e| format!("WAV passthrough copy failed: {e}"))?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Decode the first non-null audio track to per-channel f32 planes, then downmix
/// to mono. Returns (mono_samples, source_sample_rate).
fn decode_to_mono(input: &Path) -> Result<(Vec<f32>, u32), String> {
    let file = std::fs::File::open(input)
        .map_err(|e| format!("Failed to open audio file: {e}"))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = input.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| format!("Failed to probe audio format: {e}"))?;

    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| "no supported audio track".to_string())?;

    if track.codec_params.codec == CODEC_TYPE_OPUS {
        return Err("opus not yet supported".to_string());
    }

    let track_id = track.id;
    let mut src_rate = track.codec_params.sample_rate;
    // Clone codec params so the decode loop below doesn't hold an immutable borrow
    // of `format` (via `track`) while calling the mutable `format.next_packet()`.
    let codec_params = track.codec_params.clone();

    let mut decoder = symphonia::default::get_codecs()
        .make(&codec_params, &Default::default())
        .map_err(|e| format!("Failed to create audio decoder: {e}"))?;

    let mut planes: Vec<Vec<f32>> = Vec::new();
    let mut layout = Channels::empty();

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(Error::IoError(ref e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(Error::ResetRequired) => break,
            Err(_) => break,
        };
        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = decoded.spec();
                if src_rate.is_none() {
                    src_rate = Some(spec.rate);
                }
                layout |= spec.channels;
                if planes.is_empty() {
                    let nch = spec.channels.count().max(1);
                    planes = vec![Vec::new(); nch];
                }
                collect_planar(&decoded, &mut planes);
            }
            Err(Error::ResetRequired) => {
                decoder = symphonia::default::get_codecs()
                    .make(&codec_params, &Default::default())
                    .map_err(|e| format!("Failed to recreate audio decoder: {e}"))?;
            }
            Err(Error::IoError(ref e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(_) => continue,
        }
    }

    // Equalize plane lengths (defend against trailing partial frames).
    let min_len = planes.iter().map(|p| p.len()).min().unwrap_or(0);
    for p in planes.iter_mut() {
        p.truncate(min_len);
    }

    let mono = downmix_to_mono(&planes, layout);
    let rate = src_rate.unwrap_or(44_100);
    Ok((mono, rate))
}

macro_rules! accumulate_planar {
    ($planes:expr, $buf:expr, $conv:expr) => {{
        let audio_planes = $buf.planes();
        let chans = audio_planes.planes();
        for (idx, plane) in chans.iter().enumerate() {
            if let Some(out) = $planes.get_mut(idx) {
                out.reserve(plane.len());
                for &s in plane.iter() {
                    out.push($conv(s));
                }
            }
        }
    }};
}

/// Convert a decoded planar frame to normalized f32 and append into the matching
/// channel plane. Handles every Symphonia sample-type variant. Integer formats
/// are normalized by their full-scale range; F32/F64 are clamped to [-1, 1].
fn collect_planar(decoded: &AudioBufferRef, planes: &mut [Vec<f32>]) {
    use symphonia::core::sample::{i24, u24};
    match decoded {
        AudioBufferRef::U8(buf) => {
            accumulate_planar!(planes, buf, |s: u8| (s as f32 - 128.0) / 128.0)
        }
        AudioBufferRef::U16(buf) => {
            accumulate_planar!(planes, buf, |s: u16| (s as f32 - 32_768.0) / 32_768.0)
        }
        AudioBufferRef::U24(buf) => {
            accumulate_planar!(planes, buf, |s: u24| {
                (s.inner() as f32 - 8_388_608.0) / 8_388_608.0
            })
        }
        AudioBufferRef::U32(buf) => {
            accumulate_planar!(planes, buf, |s: u32| {
                (s as f32 - 2_147_483_648.0) / 2_147_483_648.0
            })
        }
        AudioBufferRef::S8(buf) => accumulate_planar!(planes, buf, |s: i8| s as f32 / 128.0),
        AudioBufferRef::S16(buf) => {
            accumulate_planar!(planes, buf, |s: i16| s as f32 / 32_768.0)
        }
        AudioBufferRef::S24(buf) => {
            // i24 wraps an i32 holding the 24-bit signed value; full-scale = 2^23.
            accumulate_planar!(planes, buf, |s: i24| s.inner() as f32 / 8_388_608.0)
        }
        AudioBufferRef::S32(buf) => {
            accumulate_planar!(planes, buf, |s: i32| s as f32 / 2_147_483_648.0)
        }
        AudioBufferRef::F32(buf) => accumulate_planar!(planes, buf, |s: f32| s.clamp(-1.0, 1.0)),
        AudioBufferRef::F64(buf) => {
            accumulate_planar!(planes, buf, |s: f64| s.clamp(-1.0, 1.0) as f32)
        }
    }
}

/// Layout-aware downmix to mono, matching ffmpeg -ac 1 semantics: LFE is
/// dropped; remaining channels are mixed with conventional coefficients (front
/// L/R at unity, centre/surround at ~-3 dB / 0.707) and normalized by the sum of
/// used weights. For unrecognized layouts, falls back to an equal-weight average
/// of all channels (LFE excluded when identifiable).
fn downmix_to_mono(planes: &[Vec<f32>], layout: Channels) -> Vec<f32> {
    let nch = planes.len();
    if nch <= 1 {
        return planes.first().cloned().unwrap_or_default();
    }

    // Symphonia orders planes by the Channels bitset, low bit first. Walk the
    // known channel flags in bit order to build per-plane weights.
    const W_FRONT: f32 = 1.0;
    const W_ATTEN: f32 = 0.707;
    const W_LFE: f32 = 0.0;
    let known: &[(Channels, f32)] = &[
        (Channels::FRONT_LEFT, W_FRONT),
        (Channels::FRONT_RIGHT, W_FRONT),
        (Channels::FRONT_CENTRE, W_ATTEN),
        (Channels::LFE1, W_LFE),
        (Channels::REAR_LEFT, W_ATTEN),
        (Channels::REAR_RIGHT, W_ATTEN),
        (Channels::FRONT_LEFT_CENTRE, W_ATTEN),
        (Channels::FRONT_RIGHT_CENTRE, W_ATTEN),
        (Channels::REAR_CENTRE, W_ATTEN),
        (Channels::SIDE_LEFT, W_ATTEN),
        (Channels::SIDE_RIGHT, W_ATTEN),
    ];

    let mut weights: Vec<f32> = Vec::with_capacity(nch);
    for &(flag, w) in known {
        if layout.contains(flag) {
            weights.push(w);
        }
    }

    // If any channel position is unrecognized (e.g. a Top channel we don't
    // model), fall back to equal-weight average; if LFE is identifiable, give
    // it weight 0 so it doesn't pollute the mix.
    if weights.len() != nch {
        weights = vec![1.0; nch];
        if let Some(lfe_idx) = lfe_plane_index(layout) {
            if lfe_idx < nch {
                weights[lfe_idx] = 0.0;
            }
        }
    }

    let wsum: f32 = weights.iter().copied().sum();
    let len = planes.iter().map(|p| p.len()).min().unwrap_or(0);
    let mut out = Vec::with_capacity(len);
    if wsum <= 0.0 {
        // Everything was LFE / silenced — emit silence rather than divide by zero.
        out.resize(len, 0.0);
        return out;
    }
    out.extend((0..len).map(|i| {
        let acc: f32 = weights
            .iter()
            .zip(planes.iter())
            .map(|(&w, p)| w * p[i])
            .sum();
        acc / wsum
    }));
    out
}

/// Index of the LFE plane in Symphonia's plane ordering, if LFE is in the
/// layout. Symphonia orders planes low-bit-first, so LFE's index is the count of
/// channel bits set below it.
fn lfe_plane_index(layout: Channels) -> Option<usize> {
    if !layout.contains(Channels::LFE1) {
        return None;
    }
    let lower_bits = layout.bits() & (Channels::LFE1.bits() - 1);
    Some(lower_bits.count_ones() as usize)
}

/// Write the samples as a 16 kHz / mono / s16 PCM WAV at output, atomically.
///
/// Writes to a sibling temp file first and renames on success, so a decode or IO
/// error after the writer is open never leaves a half-written file at output.
/// On any error the temp file is removed.
fn write_s16_wav(samples: &[f32], output: &Path) -> Result<(), String> {
    let dir = output.parent().unwrap_or_else(|| Path::new("."));
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let temp_path = dir.join(format!(".voicetypr_decode_tmp_{stamp}.wav"));

    if let Err(e) = write_s16_wav_to(samples, &temp_path) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(e);
    }
    if let Err(e) = std::fs::rename(&temp_path, output) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(format!("Failed to rename temp WAV to output: {e}"));
    }
    Ok(())
}

fn write_s16_wav_to(samples: &[f32], path: &Path) -> Result<(), String> {
    let spec = WavSpec {
        channels: TARGET_CHANNELS,
        sample_rate: TARGET_RATE,
        bits_per_sample: TARGET_BITS,
        sample_format: SampleFormat::Int,
    };
    let mut writer =
        WavWriter::create(path, spec).map_err(|e| format!("WAV create failed: {e}"))?;
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        writer
            .write_sample(v)
            .map_err(|e| format!("WAV write failed: {e}"))?;
    }
    writer
        .finalize()
        .map_err(|e| format!("WAV finalize failed: {e}"))?;
    Ok(())
}
