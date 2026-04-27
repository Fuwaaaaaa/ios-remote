//! WASAPI loopback / mic capture and the transcription pump that drains
//! captured PCM into [`super::audio_transcription::Transcriber`].
//!
//! cpal exposes WASAPI loopback by calling `build_input_stream` on the
//! default *output* device. We mirror the [`super::h264_encoder::H264Encoder`]
//! shape: `AudioCapture::new(bus, source).spawn()` returns a handle that
//! holds the cpal `Stream` for the process lifetime (drop = stop). The
//! stream itself is `!Send`, so it is owned by a parked OS thread.

use super::audio_transcription::Transcriber;
use cpal::SampleFormat;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tracing::{info, warn};

/// User-facing capture source selection. `Off` means no stream is opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioSource {
    Loopback,
    Mic,
    Off,
}

impl AudioSource {
    pub fn as_str(self) -> &'static str {
        match self {
            AudioSource::Loopback => "loopback",
            AudioSource::Mic => "mic",
            AudioSource::Off => "off",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "loopback" => Some(AudioSource::Loopback),
            "mic" | "microphone" => Some(AudioSource::Mic),
            "off" | "none" | "disabled" => Some(AudioSource::Off),
            _ => None,
        }
    }
}

/// One callback's worth of interleaved f32 PCM at `sample_rate` with
/// `channels` channels.
#[derive(Clone, Debug)]
pub struct AudioChunk {
    pub samples: Vec<f32>,
    pub channels: u16,
    pub sample_rate: u32,
}

/// Broadcast bus for captured audio. Mirrors [`super::FrameBus`] semantics
/// so multiple consumers (transcription pump, future visualizer) can
/// subscribe independently.
#[derive(Clone)]
pub struct AudioBus {
    sender: broadcast::Sender<Arc<AudioChunk>>,
}

impl AudioBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(64);
        Self { sender }
    }
    pub fn publish(&self, chunk: AudioChunk) {
        let _ = self.sender.send(Arc::new(chunk));
    }
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<AudioChunk>> {
        self.sender.subscribe()
    }
}

impl Default for AudioBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Held by `main` for the lifetime of the process. Dropping the handle
/// unparks the capture thread, which then drops the cpal `Stream`.
pub struct CaptureHandle {
    thread: Option<std::thread::JoinHandle<()>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    pub source: AudioSource,
    pub sample_rate: u32,
    pub channels: u16,
}

impl Drop for CaptureHandle {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            t.thread().unpark();
            let _ = t.join();
        }
    }
}

pub struct AudioCapture {
    bus: AudioBus,
    source: AudioSource,
}

impl AudioCapture {
    pub fn new(bus: AudioBus, source: AudioSource) -> Self {
        Self { bus, source }
    }

    /// Spawn the capture thread. Returns `None` for `AudioSource::Off`, or
    /// when no usable device exists. Loopback failure falls through to mic
    /// with a single warn, matching the roadmap's "WASAPI loopback with mic
    /// fallback" requirement.
    pub fn spawn(self) -> Option<CaptureHandle> {
        if self.source == AudioSource::Off {
            return None;
        }

        let (tx, rx) = std::sync::mpsc::channel::<Result<(AudioSource, u32, u16), String>>();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_thread = stop.clone();

        let bus = self.bus.clone();
        let requested = self.source;
        let thread = std::thread::Builder::new()
            .name("audio-capture".to_string())
            .spawn(move || {
                let outcome = build_stream(bus, requested);
                match outcome {
                    Ok((stream, source, sample_rate, channels)) => {
                        if let Err(e) = stream.play() {
                            let _ = tx.send(Err(format!("stream.play: {e}")));
                            return;
                        }
                        let _ = tx.send(Ok((source, sample_rate, channels)));
                        info!(
                            source = source.as_str(),
                            sample_rate, channels, "Audio capture started"
                        );
                        // Park until the handle is dropped; cpal Stream is !Send so
                        // the only way to keep it alive is to keep this thread alive.
                        while !stop_thread.load(std::sync::atomic::Ordering::SeqCst) {
                            std::thread::park();
                        }
                        drop(stream);
                        info!("Audio capture stopped");
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e));
                    }
                }
            })
            .ok()?;

        match rx.recv() {
            Ok(Ok((source, sample_rate, channels))) => Some(CaptureHandle {
                thread: Some(thread),
                stop,
                source,
                sample_rate,
                channels,
            }),
            Ok(Err(e)) => {
                warn!(error = %e, "Audio capture failed to start");
                None
            }
            Err(_) => {
                warn!("Audio capture thread exited before signaling");
                None
            }
        }
    }
}

/// Build a cpal stream for the requested source. Tries loopback first when
/// requested; on any failure (no default output, build_input_stream rejects
/// the format, etc.) falls back to the default input device.
fn build_stream(
    bus: AudioBus,
    requested: AudioSource,
) -> Result<(cpal::Stream, AudioSource, u32, u16), String> {
    let host = cpal::default_host();

    if requested == AudioSource::Loopback {
        match host.default_output_device() {
            Some(device) => match open_input_stream(&device, bus.clone()) {
                Ok((stream, sr, ch)) => return Ok((stream, AudioSource::Loopback, sr, ch)),
                Err(e) => warn!(error = %e, "Loopback open failed; falling back to mic"),
            },
            None => warn!("No default output device for loopback; falling back to mic"),
        }
    }

    let mic = host
        .default_input_device()
        .ok_or_else(|| "no default input device".to_string())?;
    let (stream, sr, ch) = open_input_stream(&mic, bus)?;
    Ok((stream, AudioSource::Mic, sr, ch))
}

fn open_input_stream(
    device: &cpal::Device,
    bus: AudioBus,
) -> Result<(cpal::Stream, u32, u16), String> {
    let config = device
        .default_input_config()
        .map_err(|e| format!("default_input_config: {e}"))?;
    let sample_rate = config.sample_rate().0;
    let channels = config.channels();
    let stream_config: cpal::StreamConfig = config.clone().into();
    let err_fn = |err| warn!(?err, "audio capture stream error");

    let stream = match config.sample_format() {
        SampleFormat::F32 => device.build_input_stream(
            &stream_config,
            move |data: &[f32], _: &_| {
                bus.publish(AudioChunk {
                    samples: data.to_vec(),
                    channels,
                    sample_rate,
                });
            },
            err_fn,
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            &stream_config,
            move |data: &[i16], _: &_| {
                let samples: Vec<f32> = data.iter().map(|&s| s as f32 / 32768.0).collect();
                bus.publish(AudioChunk {
                    samples,
                    channels,
                    sample_rate,
                });
            },
            err_fn,
            None,
        ),
        SampleFormat::U16 => device.build_input_stream(
            &stream_config,
            move |data: &[u16], _: &_| {
                let samples: Vec<f32> = data
                    .iter()
                    .map(|&s| (s as f32 - 32768.0) / 32768.0)
                    .collect();
                bus.publish(AudioChunk {
                    samples,
                    channels,
                    sample_rate,
                });
            },
            err_fn,
            None,
        ),
        other => return Err(format!("unsupported sample format: {other:?}")),
    }
    .map_err(|e| format!("build_input_stream: {e}"))?;

    Ok((stream, sample_rate, channels))
}

/// Drain the [`AudioBus`], down-mix to mono, resample to 16 kHz, and feed
/// `chunk_secs`-second windows into [`Transcriber::transcribe_pcm`].
pub fn spawn_transcription_pump(
    bus: AudioBus,
    transcriber: Arc<Mutex<Transcriber>>,
    chunk_secs: u32,
) {
    let chunk_secs = chunk_secs.max(1);
    tokio::spawn(async move {
        let mut rx = bus.subscribe();
        let target_rate = 16_000u32;
        let target_samples = chunk_secs as usize * target_rate as usize;
        let mut buffer: Vec<f32> = Vec::with_capacity(target_samples);
        let mut current_rate: u32 = 0;
        let mut current_channels: u16 = 0;
        let mut warned_rate_mismatch = false;

        loop {
            match rx.recv().await {
                Ok(chunk) => {
                    let reset =
                        chunk.sample_rate != current_rate || chunk.channels != current_channels;
                    if reset {
                        current_rate = chunk.sample_rate;
                        current_channels = chunk.channels.max(1);
                        buffer.clear();
                        if current_rate < target_rate && !warned_rate_mismatch {
                            warn!(
                                sample_rate = current_rate,
                                target = target_rate,
                                "audio source rate below 16 kHz; transcription quality may degrade"
                            );
                            warned_rate_mismatch = true;
                        } else if current_rate > target_rate
                            && current_rate.is_multiple_of(target_rate)
                        {
                            // exact integer ratio — best path
                        } else if !warned_rate_mismatch {
                            warn!(
                                sample_rate = current_rate,
                                target = target_rate,
                                "audio source rate is not an integer multiple of 16 kHz; using nearest-sample resample"
                            );
                            warned_rate_mismatch = true;
                        }
                    }

                    let ch = current_channels as usize;
                    let mono: Vec<f32> = chunk
                        .samples
                        .chunks_exact(ch)
                        .map(|frame| frame.iter().sum::<f32>() / ch as f32)
                        .collect();

                    if current_rate >= target_rate {
                        let step = current_rate as f64 / target_rate as f64;
                        let mut pos: f64 = 0.0;
                        while (pos as usize) < mono.len() {
                            buffer.push(mono[pos as usize]);
                            pos += step;
                        }
                    } else {
                        // Lower-rate sources (e.g. 8 kHz mic) — pass through
                        // verbatim; whisper handles them but quality drops.
                        buffer.extend_from_slice(&mono);
                    }

                    while buffer.len() >= target_samples {
                        let take: Vec<f32> = buffer.drain(..target_samples).collect();
                        // Capture the timestamp under a tiny lock so the
                        // pump never holds the Mutex across the heavy
                        // inference call. The display loop and /api/*
                        // handlers also lock this Mutex, so a multi-second
                        // hold here would freeze the UI.
                        let ts_ms = {
                            let t = transcriber.lock().unwrap_or_else(|p| p.into_inner());
                            t.now_ms()
                        };
                        let openai_key = std::env::var("OPENAI_API_KEY").ok();
                        let result = tokio::task::spawn_blocking(move || {
                            super::audio_transcription::transcribe_blocking(&take, openai_key)
                        })
                        .await;
                        match result {
                            Ok(Ok(text)) if !text.is_empty() => {
                                let mut t = transcriber.lock().unwrap_or_else(|p| p.into_inner());
                                t.add_subtitle(&text, ts_ms);
                                info!(text = %text, "transcribed chunk");
                            }
                            Ok(Ok(_)) => {}
                            Ok(Err(e)) => warn!(error = %e, "transcribe failed"),
                            Err(e) => {
                                warn!(error = %e, "transcribe blocking task panicked");
                            }
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(
                        dropped = n,
                        "Transcription pump lagged — dropped audio chunks"
                    );
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_source_parse_roundtrip() {
        for src in [AudioSource::Loopback, AudioSource::Mic, AudioSource::Off] {
            assert_eq!(AudioSource::parse(src.as_str()), Some(src));
        }
        assert_eq!(AudioSource::parse("Microphone"), Some(AudioSource::Mic));
        assert_eq!(AudioSource::parse("None"), Some(AudioSource::Off));
        assert_eq!(AudioSource::parse("garbage"), None);
    }

    #[test]
    fn audio_bus_broadcasts() {
        let bus = AudioBus::new();
        let mut rx = bus.subscribe();
        bus.publish(AudioChunk {
            samples: vec![0.5, -0.5],
            channels: 1,
            sample_rate: 16_000,
        });
        let chunk = rx.try_recv().expect("recv");
        assert_eq!(chunk.samples.len(), 2);
        assert_eq!(chunk.sample_rate, 16_000);
    }
}
