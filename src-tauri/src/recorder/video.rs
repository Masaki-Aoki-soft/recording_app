//! 映像キャプチャ: Windows Graphics Capture でウィンドウ/モニタを取得し、
//! 固定フレームレートで ffmpeg (libx264) にパイプする。

use std::io::Write;
use std::path::Path;
use std::process::{Child, ChildStdin, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use log::{error, info, warn};
use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};
use windows_capture::window::Window;

use crate::ffmpeg;
use crate::models::RecordingConfig;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// キャプチャ対象
#[derive(Debug, Clone, Copy)]
pub enum CaptureTarget {
    /// ウィンドウハンドル（HWND を isize で保持）
    Window(isize),
    PrimaryMonitor,
}

/// キャプチャスレッドとペーシングスレッドで共有する最新フレーム
struct LatestFrame {
    width: u32,
    height: u32,
    data: Vec<u8>, // BGRA（パディング無し）
    seq: u64,
}

#[derive(Default)]
struct Shared {
    latest: Mutex<Option<LatestFrame>>,
    closed: AtomicBool,
}

struct CaptureHandler {
    shared: Arc<Shared>,
    scratch: Vec<u8>,
}

impl GraphicsCaptureApiHandler for CaptureHandler {
    type Flags = Arc<Shared>;
    type Error = BoxError;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Self {
            shared: ctx.flags,
            scratch: Vec::new(),
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        _capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        let buffer = frame.buffer()?;
        let (width, height) = (buffer.width(), buffer.height());
        let pixels = buffer.as_nopadding_buffer(&mut self.scratch);

        let mut latest = self.shared.latest.lock().map_err(|_| "frame lock poisoned")?;
        match latest.as_mut() {
            Some(f) if f.width == width && f.height == height => {
                f.data.copy_from_slice(pixels);
                f.seq += 1;
            }
            _ => {
                let seq = latest.as_ref().map(|f| f.seq + 1).unwrap_or(0);
                *latest = Some(LatestFrame {
                    width,
                    height,
                    data: pixels.to_vec(),
                    seq,
                });
            }
        }
        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        info!("Capture item closed");
        self.shared.closed.store(true, Ordering::SeqCst);
        Ok(())
    }
}

type Control = CaptureControl<CaptureHandler, BoxError>;

fn start_capture(target: CaptureTarget, shared: Arc<Shared>) -> Result<Control, String> {
    // 黄色い枠線を消す設定は古い Windows では失敗するので、失敗時は既定値で再試行
    let mut last_err = String::new();
    for border in [DrawBorderSettings::WithoutBorder, DrawBorderSettings::Default] {
        let result = match target {
            CaptureTarget::Window(hwnd) => {
                let window = Window::from_raw_hwnd(hwnd as *mut std::ffi::c_void);
                CaptureHandler::start_free_threaded(Settings::new(
                    window,
                    CursorCaptureSettings::WithCursor,
                    border,
                    SecondaryWindowSettings::Include,
                    MinimumUpdateIntervalSettings::Default,
                    DirtyRegionSettings::Default,
                    ColorFormat::Bgra8,
                    shared.clone(),
                ))
            }
            CaptureTarget::PrimaryMonitor => {
                let monitor =
                    Monitor::primary().map_err(|e| format!("モニタを取得できません: {}", e))?;
                CaptureHandler::start_free_threaded(Settings::new(
                    monitor,
                    CursorCaptureSettings::WithCursor,
                    border,
                    SecondaryWindowSettings::Default,
                    MinimumUpdateIntervalSettings::Default,
                    DirtyRegionSettings::Default,
                    ColorFormat::Bgra8,
                    shared.clone(),
                ))
            }
        };
        match result {
            Ok(control) => return Ok(control),
            Err(e) => {
                warn!("Failed to start capture (border={:?}): {}", border, e);
                last_err = e.to_string();
            }
        }
    }
    Err(format!("画面キャプチャを開始できませんでした: {}", last_err))
}

pub struct VideoRecorder {
    control: Option<Control>,
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
    pacer: Option<JoinHandle<()>>,
    ffmpeg: Child,
    /// 最初のフレームを ffmpeg に書き込んだ時刻（音声との同期に使用）
    pub started_at: Instant,
}

impl VideoRecorder {
    pub fn start(
        target: CaptureTarget,
        config: &RecordingConfig,
        out_path: &Path,
    ) -> Result<Self, String> {
        let shared = Arc::new(Shared::default());
        let control = start_capture(target, shared.clone())?;

        // 最初のフレームを待ってキャプチャサイズを確定する
        let deadline = Instant::now() + Duration::from_secs(10);
        let (width, height) = loop {
            if let Some(f) = shared.latest.lock().unwrap().as_ref() {
                break (f.width, f.height);
            }
            if Instant::now() > deadline {
                let _ = control.stop();
                return Err(
                    "キャプチャ対象から映像を取得できませんでした（ウィンドウが最小化されていませんか？）"
                        .into(),
                );
            }
            std::thread::sleep(Duration::from_millis(50));
        };

        let fps = config.fps();
        let (max_w, max_h) = config.max_size();
        info!(
            "Video capture {}x{} @ {}fps -> max {}x{}",
            width, height, fps, max_w, max_h
        );

        // 縮小のみ行い（拡大はしない）、偶数サイズに揃える
        let filter = format!(
            "scale=w='min({max_w},iw)':h='min({max_h},ih)':force_original_aspect_ratio=decrease:force_divisible_by=2,format=yuv420p"
        );
        let spawn = ffmpeg::command()
            .args(["-y", "-f", "rawvideo", "-pix_fmt", "bgra"])
            .args(["-video_size", &format!("{}x{}", width, height)])
            .args(["-framerate", &fps.to_string()])
            .args(["-i", "pipe:0"])
            .args(["-vf", &filter])
            .args(["-c:v", "libx264", "-preset", "veryfast", "-crf", "23"])
            .args(["-f", "matroska"])
            .arg(out_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        let mut child = match spawn {
            Ok(child) => child,
            Err(e) => {
                let _ = control.stop();
                return Err(format!("FFmpeg を起動できませんでした: {}", e));
            }
        };

        let stdin = child
            .stdin
            .take()
            .ok_or("FFmpeg の標準入力を取得できません")?;
        let stop = Arc::new(AtomicBool::new(false));
        let started_at = Instant::now();

        let pacer = {
            let shared = shared.clone();
            let stop = stop.clone();
            std::thread::Builder::new()
                .name("video-pacer".into())
                .spawn(move || pace_frames(stdin, shared, stop, width, height, fps, started_at))
                .map_err(|e| format!("ペーシングスレッドを起動できません: {}", e))?
        };

        Ok(Self {
            control: Some(control),
            shared,
            stop,
            pacer: Some(pacer),
            ffmpeg: child,
            started_at,
        })
    }

    /// エンコーダ（ffmpeg）とペーシングスレッドが生きているか
    pub fn is_alive(&mut self) -> bool {
        let ffmpeg_alive = matches!(self.ffmpeg.try_wait(), Ok(None));
        let pacer_alive = self
            .pacer
            .as_ref()
            .map(|h| !h.is_finished())
            .unwrap_or(false);
        ffmpeg_alive && pacer_alive
    }

    /// キャプチャ対象（ウィンドウ）が閉じられたか
    pub fn is_target_closed(&self) -> bool {
        self.shared.closed.load(Ordering::SeqCst)
    }

    /// 録画を停止して ffmpeg の終了を待つ
    pub fn stop(mut self) -> Result<(), String> {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(pacer) = self.pacer.take() {
            let _ = pacer.join();
        }
        if let Some(control) = self.control.take() {
            if let Err(e) = control.stop() {
                warn!("Failed to stop capture cleanly: {}", e);
            }
        }

        // stdin が閉じられたので ffmpeg はファイルを書き終えて終了する
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            match self.ffmpeg.try_wait() {
                Ok(Some(status)) if status.success() => return Ok(()),
                Ok(Some(status)) => {
                    return Err(format!("FFmpeg（映像）が異常終了しました: {}", status))
                }
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(100))
                }
                Ok(None) => {
                    let _ = self.ffmpeg.kill();
                    return Err("FFmpeg（映像）の終了待ちがタイムアウトしました".into());
                }
                Err(e) => return Err(format!("FFmpeg の状態を取得できません: {}", e)),
            }
        }
    }
}

impl Drop for VideoRecorder {
    fn drop(&mut self) {
        // stop() を経由せずに破棄された場合の後始末
        self.stop.store(true, Ordering::SeqCst);
        if let Some(control) = self.control.take() {
            let _ = control.stop();
        }
        if let Some(pacer) = self.pacer.take() {
            let _ = pacer.join();
        }
    }
}

/// 壁時計に合わせて固定 FPS でフレームを書き込む。
/// 新しいフレームが無ければ直前のフレームを繰り返し、遅れた場合は同じフレームを複数回書いて追いつく。
fn pace_frames(
    mut stdin: ChildStdin,
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
    width: u32,
    height: u32,
    fps: u32,
    started_at: Instant,
) {
    let frame_len = (width * height * 4) as usize;
    let mut out = vec![0u8; frame_len];
    let mut last_seq = u64::MAX;
    let mut written: u64 = 0;
    let max_catch_up = fps as u64 * 2;

    while !stop.load(Ordering::SeqCst) {
        // 最新フレームを出力バッファへ（サイズが変わっていたら縮尺を合わせる）
        if let Ok(latest) = shared.latest.lock() {
            if let Some(f) = latest.as_ref() {
                if f.seq != last_seq {
                    if f.width == width && f.height == height {
                        out.copy_from_slice(&f.data);
                    } else {
                        resize_letterbox(&f.data, f.width, f.height, &mut out, width, height);
                    }
                    last_seq = f.seq;
                }
            }
        }

        // 経過時間から本来書き込まれているべきフレーム数を求め、不足分を書く
        let due = (started_at.elapsed().as_secs_f64() * fps as f64) as u64 + 1;
        let mut to_write = due.saturating_sub(written);
        if to_write > max_catch_up {
            // PC のスリープ等で大幅に遅れた場合は 2 秒分だけ埋めて残りは諦める
            written = due - max_catch_up;
            to_write = max_catch_up;
        }
        for _ in 0..to_write {
            if let Err(e) = stdin.write_all(&out) {
                error!("Failed to write frame to ffmpeg: {}", e);
                return;
            }
        }
        written += to_write;

        let next = started_at + Duration::from_secs_f64(written as f64 / fps as f64);
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        }
    }
    // stdin を drop して ffmpeg に EOF を伝える
}

/// 最近傍補間でアスペクト比を保って縮尺し、余白を黒で埋める
fn resize_letterbox(src: &[u8], sw: u32, sh: u32, dst: &mut [u8], dw: u32, dh: u32) {
    dst.fill(0);
    if sw == 0 || sh == 0 {
        return;
    }
    let scale = f64::min(dw as f64 / sw as f64, dh as f64 / sh as f64);
    let tw = ((sw as f64 * scale) as u32).clamp(1, dw);
    let th = ((sh as f64 * scale) as u32).clamp(1, dh);
    let ox = (dw - tw) / 2;
    let oy = (dh - th) / 2;
    for y in 0..th {
        let sy = ((y as u64 * sh as u64) / th as u64) as usize;
        let src_row = &src[sy * sw as usize * 4..(sy + 1) * sw as usize * 4];
        let dst_start = ((oy + y) as usize * dw as usize + ox as usize) * 4;
        let dst_row = &mut dst[dst_start..dst_start + tw as usize * 4];
        for x in 0..tw as usize {
            let sx = (x as u64 * sw as u64 / tw as u64) as usize;
            dst_row[x * 4..x * 4 + 4].copy_from_slice(&src_row[sx * 4..sx * 4 + 4]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::resize_letterbox;

    #[test]
    fn letterbox_keeps_aspect_and_centers() {
        // 2x1 の白画像を 4x4 に収めると、上下に黒帯が入り中央 2 行が白になる
        let src = vec![255u8; 2 * 4];
        let mut dst = vec![7u8; 4 * 4 * 4];
        resize_letterbox(&src, 2, 1, &mut dst, 4, 4);
        let row = |y: usize| &dst[y * 16..(y + 1) * 16];
        assert!(row(0).iter().all(|&b| b == 0));
        assert!(row(1).iter().all(|&b| b == 255));
        assert!(row(2).iter().all(|&b| b == 255));
        assert!(row(3).iter().all(|&b| b == 0));
    }
}
