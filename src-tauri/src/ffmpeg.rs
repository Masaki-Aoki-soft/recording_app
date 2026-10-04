//! 同梱の FFmpeg（sidecar）の起動ヘルパー

use std::path::PathBuf;
use std::process::Command;

/// 同梱 ffmpeg.exe のパス。
/// Tauri は externalBin を実行ファイルと同じディレクトリへ（ターゲットトリプル無しの名前で）配置する。
/// 見つからない場合は PATH 上の ffmpeg にフォールバックする。
pub fn ffmpeg_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("ffmpeg.exe")))
        .filter(|path| path.exists())
        .unwrap_or_else(|| PathBuf::from("ffmpeg"))
}

/// コンソールウィンドウを出さずに ffmpeg を起動する Command を作成
pub fn command() -> Command {
    let mut cmd = Command::new(ffmpeg_path());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.arg("-hide_banner").arg("-loglevel").arg("error");
    cmd
}
