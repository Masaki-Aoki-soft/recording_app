//! 録画エンジン
//!
//! 映像（WGC → ffmpeg → 一時 mkv）と音声（WASAPI → 一時 wav）を別々に記録し、
//! 停止時に ffmpeg で開始時刻差を補正して 1 本の mp4 に mux する。

pub mod audio;
pub mod video;

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Instant;

use log::{info, warn};

use crate::ffmpeg;
use crate::models::RecordingConfig;
use audio::{AudioRecorder, AudioSource};
pub use video::CaptureTarget;
use video::VideoRecorder;

pub struct Recording {
    video: VideoRecorder,
    loopback: Option<AudioRecorder>,
    mic: Option<AudioRecorder>,
    temp_dir: PathBuf,
    output_path: PathBuf,
}

impl Recording {
    /// 録画を開始する。音声デバイスの失敗は警告に留め、映像のみでも録画を続ける
    pub fn start(
        target: CaptureTarget,
        config: &RecordingConfig,
        output_path: &Path,
    ) -> Result<(Self, Vec<String>), String> {
        let temp_dir = output_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(".tmp")
            .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&temp_dir)
            .map_err(|e| format!("一時フォルダを作成できません: {}", e))?;

        let mut warnings = Vec::new();

        // 音声を先に開始（映像の最初のフレーム待ちの間の音も拾うため）
        let loopback = if config.capture_system_audio {
            match AudioRecorder::start(AudioSource::SystemLoopback, &temp_dir.join("system.wav")) {
                Ok(r) => Some(r),
                Err(e) => {
                    warnings.push(format!("システム音声を録音できません: {}", e));
                    None
                }
            }
        } else {
            None
        };
        let mic = if config.capture_mic {
            match AudioRecorder::start(
                AudioSource::Microphone(config.mic_device.clone()),
                &temp_dir.join("mic.wav"),
            ) {
                Ok(r) => Some(r),
                Err(e) => {
                    warnings.push(format!("マイクを録音できません: {}", e));
                    None
                }
            }
        } else {
            None
        };

        let video = match VideoRecorder::start(target, config, &temp_dir.join("video.mkv")) {
            Ok(v) => v,
            Err(e) => {
                drop(loopback);
                drop(mic);
                let _ = std::fs::remove_dir_all(&temp_dir);
                return Err(e);
            }
        };

        for w in &warnings {
            warn!("{}", w);
        }

        Ok((
            Self {
                video,
                loopback,
                mic,
                temp_dir,
                output_path: output_path.to_path_buf(),
            },
            warnings,
        ))
    }

    pub fn is_alive(&mut self) -> bool {
        self.video.is_alive()
    }

    pub fn is_target_closed(&self) -> bool {
        self.video.is_target_closed()
    }

    /// 録画を停止し、mp4 に変換して保存先パスを返す
    pub fn stop_and_finalize(self) -> Result<PathBuf, String> {
        let Recording {
            video,
            loopback,
            mic,
            temp_dir,
            output_path,
        } = self;

        let video_start = video.started_at;
        let video_result = video.stop();

        let mut audio_inputs: Vec<(PathBuf, Instant)> = Vec::new();
        for recorder in [loopback, mic].into_iter().flatten() {
            let started = recorder.started_at;
            match recorder.stop() {
                Ok(path) => audio_inputs.push((path, started)),
                Err(e) => warn!("Audio track dropped: {}", e),
            }
        }

        video_result?;

        let video_path = temp_dir.join("video.mkv");
        mux(&video_path, video_start, &audio_inputs, &output_path).map_err(|e| {
            format!(
                "{}（素材ファイルは {} に残しています）",
                e,
                temp_dir.display()
            )
        })?;

        if let Err(e) = std::fs::remove_dir_all(&temp_dir) {
            warn!("Failed to remove temp dir {}: {}", temp_dir.display(), e);
        }
        info!("Recording saved: {}", output_path.display());
        Ok(output_path)
    }
}

/// 映像と音声を開始時刻差を補正して mp4 に mux する
fn mux(
    video: &Path,
    video_start: Instant,
    audio: &[(PathBuf, Instant)],
    output: &Path,
) -> Result<(), String> {
    let base = audio
        .iter()
        .map(|(_, t)| *t)
        .chain(std::iter::once(video_start))
        .min()
        .unwrap_or(video_start);
    let offset = |t: Instant| format!("{:.3}", t.duration_since(base).as_secs_f64());

    let mut cmd = ffmpeg::command();
    cmd.arg("-y");
    cmd.args(["-itsoffset", &offset(video_start), "-i"]).arg(video);
    for (path, start) in audio {
        cmd.args(["-itsoffset", &offset(*start), "-i"]).arg(path);
    }

    cmd.args(["-map", "0:v"]);
    match audio.len() {
        0 => {}
        1 => {
            cmd.args(["-map", "1:a"]);
        }
        n => {
            let inputs: String = (1..=n).map(|i| format!("[{}:a]", i)).collect();
            cmd.args([
                "-filter_complex",
                &format!(
                    "{}amix=inputs={}:duration=longest:normalize=0[aout]",
                    inputs, n
                ),
                "-map",
                "[aout]",
            ]);
        }
    }
    if !audio.is_empty() {
        cmd.args(["-c:a", "aac", "-b:a", "160k"]);
    }
    cmd.args(["-c:v", "copy", "-movflags", "+faststart"])
        .arg(output)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let out = cmd
        .output()
        .map_err(|e| format!("FFmpeg を起動できませんでした: {}", e))?;
    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        Err(format!(
            "動画ファイルの生成に失敗しました: {}",
            stderr.lines().last().unwrap_or("unknown error")
        ))
    }
}
