use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc as std_mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::Serialize;
use tokio::sync::mpsc;

use crate::projects::types::TrackRole;
use crate::realtime::AudioFrame;

const SOUND_CHECK_MAX_SAMPLES: usize = 96_000;
const SOUND_CHECK_DURATION: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDeviceInfo {
    pub id: String,
    pub label: String,
    pub is_default: bool,
    pub permission_status: String,
    pub available: bool,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoundCheck {
    pub level: f32,
    pub peak: f32,
    pub low_volume: bool,
    pub excessive_noise: bool,
    pub clipping: bool,
    pub status: String,
}

/// Send-safe control handle for an input stream.
///
/// `cpal::Stream` is deliberately created, owned, and dropped inside the
/// dedicated capture thread because CoreAudio streams are not `Send` on macOS.
/// Tauri state stores only this stop sender and thread join handle.
pub struct ActiveCapture {
    stop_tx: Option<std_mpsc::Sender<()>>,
    join: Option<JoinHandle<()>>,
}

/// A started microphone capture and its asynchronous runtime-error channel.
///
/// Stream startup errors are returned from `start_capture`. Errors reported by
/// CoreAudio/CPAL after startup are sent through `runtime_errors` so the
/// realtime pipeline can fail closed instead of silently waiting for audio.
pub struct StartedCapture {
    pub handle: ActiveCapture,
    pub runtime_errors: mpsc::UnboundedReceiver<&'static str>,
}

impl ActiveCapture {
    pub fn stop(&mut self) {
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(());
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for ActiveCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn input_devices() -> Result<Vec<AudioDeviceInfo>, &'static str> {
    let host = cpal::default_host();
    let default_name = host
        .default_input_device()
        .and_then(|device| device.name().ok());
    let devices = host.input_devices().map_err(|_| "ERR_AUDIO_PERMISSION")?;
    let mut output = Vec::new();
    for (index, device) in devices.enumerate() {
        let label = device.name().unwrap_or_else(|_| format!("Input {index}"));
        let config = device.default_input_config().ok();
        output.push(AudioDeviceInfo {
            id: format!("input-{index}"),
            is_default: default_name.as_deref() == Some(label.as_str()),
            label,
            permission_status: "granted".into(),
            available: config.is_some(),
            sample_rate: config.as_ref().map(|value| value.sample_rate().0),
            channels: config.as_ref().map(|value| value.channels()),
        });
    }
    Ok(output)
}

pub async fn sound_check(device_id: &str) -> Result<SoundCheck, &'static str> {
    let device_id = device_id.to_owned();
    tokio::task::spawn_blocking(move || sound_check_blocking(&device_id))
        .await
        .map_err(|_| "ERR_AUDIO_DEVICE_UNAVAILABLE")?
}

fn sound_check_blocking(device_id: &str) -> Result<SoundCheck, &'static str> {
    let samples = Arc::new(Mutex::new(Vec::<i16>::new()));
    let sink = samples.clone();
    let runtime_failed = Arc::new(AtomicBool::new(false));
    let runtime_failed_callback = runtime_failed.clone();
    let stream = build_stream(
        device_id,
        move |data, _rate, _channels| {
            if let Ok(mut target) = sink.lock() {
                let remaining = SOUND_CHECK_MAX_SAMPLES.saturating_sub(target.len());
                target.extend(data.into_iter().take(remaining));
            }
        },
        move || {
            runtime_failed_callback.store(true, Ordering::Relaxed);
        },
    )?;
    stream.play().map_err(|_| "ERR_AUDIO_DEVICE_UNAVAILABLE")?;
    std::thread::sleep(SOUND_CHECK_DURATION);
    drop(stream);

    if runtime_failed.load(Ordering::Relaxed) {
        return Err("ERR_AUDIO_RUNTIME");
    }
    let samples = samples.lock().map_err(|_| "ERR_STATE")?;
    summarize_samples(&samples)
}

pub fn start_capture(
    device_id: &str,
    track_role: TrackRole,
    sender: mpsc::Sender<AudioFrame>,
) -> Result<StartedCapture, &'static str> {
    let device_id = device_id.to_owned();
    let (stop_tx, stop_rx) = std_mpsc::channel::<()>();
    let (startup_tx, startup_rx) = std_mpsc::sync_channel::<Result<(), &'static str>>(1);
    let (runtime_error_tx, runtime_errors) = mpsc::unbounded_channel::<&'static str>();

    let runtime_stop_tx = stop_tx.clone();
    let closed_receiver_stop_tx = stop_tx.clone();
    let join = std::thread::Builder::new()
        .name("accordmesh-audio-capture".into())
        .spawn(move || {
            let started = Instant::now();
            let runtime_error_tx_for_stream = runtime_error_tx.clone();
            let stream = match build_stream(
                &device_id,
                move |data, sample_rate, channels| {
                    let frame = AudioFrame {
                        track_role,
                        timestamp_ms: started.elapsed().as_millis() as i64,
                        sample_rate,
                        channels,
                        mono_pcm: to_mono(&data, channels),
                    };
                    if sender.try_send(frame).is_err() && sender.is_closed() {
                        let _ = closed_receiver_stop_tx.send(());
                    }
                },
                move || {
                    let _ = runtime_error_tx_for_stream.send("ERR_AUDIO_RUNTIME");
                    let _ = runtime_stop_tx.send(());
                },
            ) {
                Ok(stream) => stream,
                Err(code) => {
                    let _ = startup_tx.send(Err(code));
                    return;
                }
            };

            if stream.play().is_err() {
                let _ = startup_tx.send(Err("ERR_AUDIO_DEVICE_UNAVAILABLE"));
                return;
            }
            if startup_tx.send(Ok(())).is_err() {
                return;
            }

            // Keep both stream creation and destruction on this thread.
            let _ = stop_rx.recv();
            drop(stream);
            drop(runtime_error_tx);
        })
        .map_err(|_| "ERR_AUDIO_DEVICE_UNAVAILABLE")?;

    match startup_rx.recv() {
        Ok(Ok(())) => Ok(StartedCapture {
            handle: ActiveCapture {
                stop_tx: Some(stop_tx),
                join: Some(join),
            },
            runtime_errors,
        }),
        Ok(Err(code)) => {
            let _ = join.join();
            Err(code)
        }
        Err(_) => {
            let _ = join.join();
            Err("ERR_AUDIO_DEVICE_UNAVAILABLE")
        }
    }
}

fn find_device(device_id: &str) -> Result<cpal::Device, &'static str> {
    let index = device_id
        .strip_prefix("input-")
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or("ERR_AUDIO_DEVICE_UNAVAILABLE")?;
    cpal::default_host()
        .input_devices()
        .map_err(|_| "ERR_AUDIO_PERMISSION")?
        .nth(index)
        .ok_or("ERR_AUDIO_DEVICE_UNAVAILABLE")
}

fn build_stream<F, E>(
    device_id: &str,
    callback: F,
    runtime_error: E,
) -> Result<cpal::Stream, &'static str>
where
    F: Fn(Vec<i16>, u32, u16) + Send + Sync + 'static,
    E: Fn() + Send + Sync + 'static,
{
    let device = find_device(device_id)?;
    let supported = device
        .default_input_config()
        .map_err(|_| "ERR_AUDIO_DEVICE_UNAVAILABLE")?;
    let sample_rate = supported.sample_rate().0;
    let channels = supported.channels();
    let callback = Arc::new(callback);
    let runtime_error = Arc::new(runtime_error);
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => {
            let callback = callback.clone();
            let runtime_error = runtime_error.clone();
            device.build_input_stream(
                &supported.config(),
                move |data: &[f32], _| {
                    callback(
                        data.iter()
                            .map(|value| (*value * 32767.0).clamp(-32768.0, 32767.0) as i16)
                            .collect(),
                        sample_rate,
                        channels,
                    )
                },
                move |_error| runtime_error(),
                None,
            )
        }
        cpal::SampleFormat::I16 => {
            let callback = callback.clone();
            let runtime_error = runtime_error.clone();
            device.build_input_stream(
                &supported.config(),
                move |data: &[i16], _| callback(data.to_vec(), sample_rate, channels),
                move |_error| runtime_error(),
                None,
            )
        }
        cpal::SampleFormat::U16 => {
            let callback = callback.clone();
            let runtime_error = runtime_error.clone();
            device.build_input_stream(
                &supported.config(),
                move |data: &[u16], _| {
                    callback(
                        data.iter()
                            .map(|value| (*value as i32 - 32768) as i16)
                            .collect(),
                        sample_rate,
                        channels,
                    )
                },
                move |_error| runtime_error(),
                None,
            )
        }
        _ => return Err("ERR_AUDIO_FORMAT"),
    }
    .map_err(|_| "ERR_AUDIO_DEVICE_UNAVAILABLE")?;
    Ok(stream)
}

fn summarize_samples(samples: &[i16]) -> Result<SoundCheck, &'static str> {
    if samples.is_empty() {
        return Err("ERR_AUDIO_NO_SIGNAL");
    }
    let peak = samples
        .iter()
        .map(|value| value.unsigned_abs() as f32 / 32768.0)
        .fold(0.0, f32::max);
    let rms = (samples
        .iter()
        .map(|value| {
            let normalized = *value as f64 / 32768.0;
            normalized * normalized
        })
        .sum::<f64>()
        / samples.len() as f64)
        .sqrt() as f32;
    let low_volume = rms < 0.015;
    let clipping = peak > 0.985;
    let excessive_noise = rms > 0.25 && peak / rms < 2.0;
    let status = if clipping {
        "clipping"
    } else if low_volume {
        "low_volume"
    } else if excessive_noise {
        "excessive_noise"
    } else {
        "ready"
    };
    Ok(SoundCheck {
        level: rms,
        peak,
        low_volume,
        excessive_noise,
        clipping,
        status: status.into(),
    })
}

fn to_mono(samples: &[i16], channels: u16) -> Vec<i16> {
    if channels <= 1 {
        return samples.to_vec();
    }
    samples
        .chunks(channels as usize)
        .map(|chunk| {
            (chunk.iter().map(|value| *value as i32).sum::<i32>() / chunk.len() as i32) as i16
        })
        .collect()
}

#[cfg(test)]
pub(crate) fn summarize_samples_for_test(samples: &[i16]) -> Result<SoundCheck, &'static str> {
    summarize_samples(samples)
}

#[cfg(test)]
pub(crate) fn to_mono_for_test(samples: &[i16], channels: u16) -> Vec<i16> {
    to_mono(samples, channels)
}
