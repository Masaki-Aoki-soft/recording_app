use std::process::Command;
use tokio::io::AsyncWriteExt;
use tokio::net::windows::named_pipe::ServerOptions;

#[tokio::main]
async fn main() {
    let pipe_name = r"\\.\pipe\test_pcm_pipe";
    let mut server = ServerOptions::new()
        .first_pipe_instance(true)
        .create(pipe_name)
        .unwrap();

    let mut ffmpeg = Command::new("c:/Users/amas4/Desktop/Programming/tauri/recording_app/src-tauri/bin/ffmpeg-x86_64-pc-windows-msvc.exe")
        .args(&[
            "-y",
            "-f", "f32le",
            "-ar", "48000",
            "-ac", "2",
            "-i", pipe_name,
            "-t", "3",
            "test_output.wav",
        ])
        .spawn()
        .unwrap();

    println!("Waiting for ffmpeg connection...");
    server.connect().await.unwrap();
    println!("FFmpeg connected! Sending PCM data...");

    // Generate 3 seconds of 440hz sine wave at 48000hz 2ch f32le
    let mut data = Vec::new();
    for i in 0..(48000 * 3) {
        let t = i as f32 / 48000.0;
        let sample = (t * 440.0 * 2.0 * std::f32::consts::PI).sin() * 0.5;
        let bytes = sample.to_le_bytes();
        data.extend_from_slice(&bytes); // Left
        data.extend_from_slice(&bytes); // Right
    }

    server.write_all(&data).await.unwrap();
    println!("Data sent! Waiting for ffmpeg to finish...");

    ffmpeg.wait().unwrap();
}
