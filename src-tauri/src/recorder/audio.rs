//! 音声キャプチャ: WASAPI（cpal）でシステム音声のループバック / マイクを WAV に書き出す。
//!
//! WASAPI ループバックは無音時にデータが届かないため、壁時計との差分を無音で埋めて
//! 映像とのタイムラインを揃える。

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use log::{error, info, warn};

type Writer = hound::WavWriter<BufWriter<File>>;

/// 音声ソース
#[derive(Debug, Clone)]
pub enum AudioSource {
    /// 既定の出力デバイスのループバック（相手の声・システム音）
    SystemLoopback,
    /// マイク。None の場合は既定の入力デバイス
    Microphone(Option<String>),
}

/// ループバックの無音区間を埋める際の許容ずれ（これ以上遅れたら無音を挿入）
const GAP_TOLERANCE_SECS: f64 = 0.1;

struct WavState {
    writer: Option<Writer>,
    channels: u16,
    sample_rate: u32,
    frames_written: u64,
    started_at: Instant,
}

impl WavState {
    /// 壁時計上の経過時間に対して書き込み済みフレームが不足していれば無音を挿入
    fn pad_to(&mut self, elapsed_secs: f64, incoming_frames: u64) {
        let expected = (elapsed_secs * self.sample_rate as f64) as u64;
        let tolerance = (GAP_TOLERANCE_SECS * self.sample_rate as f64) as u64;
        let have = self.frames_written + incoming_frames;
        if expected > have + tolerance {
            let missing = expected - have;
            if let Some(writer) = self.writer.as_mut() {
                for _ in 0..missing * self.channels as u64 {
                    if writer.write_sample(0i16).is_err() {
                        break;
                    }
                }
            }
            self.frames_written += missing;
        }
    }

    fn write<T>(&mut self, data: &[T])
    where
        T: SizedSample,
        f32: FromSample<T>,
    {
        let channels = self.channels.max(1) as usize;
        let frames = (data.len() / channels) as u64;
        let elapsed = self.started_at.elapsed().as_secs_f64();
        self.pad_to(elapsed, frames);

        if let Some(writer) = self.writer.as_mut() {
            for &sample in data {
                let v = f32::from_sample_(sample).clamp(-1.0, 1.0);
                if writer.write_sample((v * i16::MAX as f32) as i16).is_err() {
                    break;
                }
            }
        }
        self.frames_written += frames;
    }
}

pub struct AudioRecorder {
    stop_tx: mpsc::Sender<()>,
    thread: Option<JoinHandle<Result<(), String>>>,
    pub path: PathBuf,
    /// 録音開始時刻（映像との同期に使用）
    pub started_at: Instant,
}

impl AudioRecorder {
    pub fn start(source: AudioSource, path: &Path) -> Result<Self, String> {
        let (ready_tx, ready_rx) = mpsc::channel::<Result<Instant, String>>();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let path_buf = path.to_path_buf();

        let thread = std::thread::Builder::new()
            .name("audio-capture".into())
            .spawn(move || run_capture(source, path_buf, ready_tx, stop_rx))
            .map_err(|e| format!("音声スレッドを起動できません: {}", e))?;

        match ready_rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok(started_at)) => Ok(Self {
                stop_tx,
                thread: Some(thread),
                path: path.to_path_buf(),
                started_at,
            }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => Err("音声デバイスの初期化がタイムアウトしました".into()),
        }
    }

    /// 録音を停止して WAV ファイルを確定する
    pub fn stop(mut self) -> Result<PathBuf, String> {
        let _ = self.stop_tx.send(());
        match self.thread.take().map(|t| t.join()) {
            Some(Ok(Ok(()))) => Ok(self.path.clone()),
            Some(Ok(Err(e))) => Err(e),
            Some(Err(_)) => Err("音声スレッドがパニックしました".into()),
            None => Ok(self.path.clone()),
        }
    }
}

impl Drop for AudioRecorder {
    fn drop(&mut self) {
        let _ = self.stop_tx.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn find_input_device(host: &cpal::Host, name: &str) -> Option<cpal::Device> {
    host.input_devices().ok()?.find(|d| {
        d.description()
            .map(|desc| desc.name() == name)
            .unwrap_or(false)
    })
}

/// 入力（マイク）デバイス名の一覧
pub fn list_input_devices() -> Result<Vec<String>, String> {
    let host = cpal::default_host();
    let devices = host
        .input_devices()
        .map_err(|e| format!("入力デバイスを列挙できません: {}", e))?;
    let mut names: Vec<String> = devices
        .filter_map(|d| d.description().ok().map(|desc| desc.name().to_string()))
        .collect();
    names.dedup();
    Ok(names)
}

fn run_capture(
    source: AudioSource,
    path: PathBuf,
    ready_tx: mpsc::Sender<Result<Instant, String>>,
    stop_rx: mpsc::Receiver<()>,
) -> Result<(), String> {
    let setup = || -> Result<(cpal::Stream, Arc<Mutex<WavState>>), String> {
        let host = cpal::default_host();
        let (device, supported) = match &source {
            AudioSource::SystemLoopback => {
                let device = host
                    .default_output_device()
                    .ok_or("出力デバイスが見つかりません")?;
                // WASAPI では出力デバイスで入力ストリームを作るとループバックになる
                let config = device
                    .default_output_config()
                    .map_err(|e| format!("出力デバイスの設定を取得できません: {}", e))?;
                (device, config)
            }
            AudioSource::Microphone(name) => {
                let device = name
                    .as_deref()
                    .and_then(|n| find_input_device(&host, n))
                    .or_else(|| host.default_input_device())
                    .ok_or("マイクが見つかりません")?;
                let config = device
                    .default_input_config()
                    .map_err(|e| format!("マイクの設定を取得できません: {}", e))?;
                (device, config)
            }
        };

        let device_name = device
            .description()
            .map(|d| d.name().to_string())
            .unwrap_or_default();
        info!(
            "Audio capture {:?} on '{}': {:?}",
            source, device_name, supported
        );

        let stream_config = supported.config();
        let spec = hound::WavSpec {
            channels: stream_config.channels,
            sample_rate: stream_config.sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let writer = hound::WavWriter::create(&path, spec)
            .map_err(|e| format!("WAV ファイルを作成できません: {}", e))?;

        let state = Arc::new(Mutex::new(WavState {
            writer: Some(writer),
            channels: stream_config.channels,
            sample_rate: stream_config.sample_rate,
            frames_written: 0,
            started_at: Instant::now(),
        }));

        let stream = match supported.sample_format() {
            SampleFormat::F32 => build_stream::<f32>(&device, stream_config, state.clone()),
            SampleFormat::I16 => build_stream::<i16>(&device, stream_config, state.clone()),
            SampleFormat::I32 => build_stream::<i32>(&device, stream_config, state.clone()),
            SampleFormat::U8 => build_stream::<u8>(&device, stream_config, state.clone()),
            other => Err(format!("未対応のサンプル形式です: {:?}", other)),
        }?;
        stream
            .play()
            .map_err(|e| format!("音声ストリームを開始できません: {}", e))?;
        Ok((stream, state))
    };

    let (stream, state) = match setup() {
        Ok(v) => v,
        Err(e) => {
            let _ = ready_tx.send(Err(e.clone()));
            return Err(e);
        }
    };

    // 実際の開始時刻を記録（ここから無音補完の基準にする）
    let started_at = Instant::now();
    if let Ok(mut s) = state.lock() {
        s.started_at = started_at;
        s.frames_written = 0;
    }
    let _ = ready_tx.send(Ok(started_at));

    // 停止指示（または送信側の破棄）まで待機
    let _ = stop_rx.recv();
    drop(stream);

    let mut s = state.lock().map_err(|_| "audio state lock poisoned")?;
    let elapsed = s.started_at.elapsed().as_secs_f64();
    s.pad_to(elapsed, 0);
    if let Some(writer) = s.writer.take() {
        writer
            .finalize()
            .map_err(|e| format!("WAV ファイルを確定できません: {}", e))?;
    }
    Ok(())
}

fn build_stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    state: Arc<Mutex<WavState>>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    device
        .build_input_stream::<T, _, _>(
            config,
            move |data: &[T], _info| {
                if let Ok(mut s) = state.lock() {
                    s.write(data);
                }
            },
            |err| error!("Audio stream error: {}", err),
            None,
        )
        .map_err(|e| {
            warn!("Failed to build input stream: {}", e);
            format!("音声ストリームを作成できません: {}", e)
        })
}
