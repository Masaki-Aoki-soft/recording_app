use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, SupportedStreamConfig};
use log::{error, info};
use ringbuf::traits::{Consumer, Producer, Split};
use ringbuf::HeapRb;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::windows::named_pipe::ServerOptions;

pub struct AudioState {
    pub is_running: Arc<AtomicBool>,
}

pub fn get_host_mic_devices() -> Result<Vec<String>, String> {
    let host = cpal::default_host();
    let devices = host
        .input_devices()
        .map_err(|e| format!("Failed to get input devices: {}", e))?;

    let mut names = Vec::new();
    for dev in devices {
        if let Ok(name) = dev.name() {
            names.push(name);
        }
    }
    // Loopback devices (System audio) are usually default output
    Ok(names)
}

fn get_device(name: &str, is_input: bool) -> Option<cpal::Device> {
    let host = cpal::default_host();
    let devices = if is_input {
        host.input_devices().ok()?
    } else {
        host.output_devices().ok()?
    };

    for dev in devices {
        if let Ok(n) = dev.name() {
            if n == name {
                return Some(dev);
            }
        }
    }
    None
}

/// Starts capturing from a device and writes raw PCM (`f32le`) into a Windows named pipe.
pub async fn start_capture_stream(
    device_name: String,
    is_input: bool,
    pipe_name: String,
    is_running: Arc<AtomicBool>,
) -> Result<(cpal::Stream, String, u32, u16), String> {
    let device = get_device(&device_name, is_input)
        .ok_or_else(|| format!("Device not found: {}", device_name))?;

    let config: SupportedStreamConfig = device
        .default_input_config()
        .or_else(|_| device.default_output_config())
        .map_err(|e| format!("Default config error: {}", e))?;

    let sample_rate = config.sample_rate();
    let channels = config.channels();

    // Create a ring buffer for 1 second of audio
    let ring_buf_capacity = (sample_rate * channels as u32) as usize * 4; // 1 second * 4 bytes per f32
    let rb = HeapRb::<u8>::new(ring_buf_capacity);
    let (mut prod, mut cons) = rb.split();

    // Tokio named pipe server
    let pipe_path = format!(r"\\.\pipe\{}", pipe_name);
    let mut server = ServerOptions::new()
        .first_pipe_instance(true)
        .create(&pipe_path)
        .map_err(|e| format!("Failed to create named pipe: {}", e))?;

    // Create a Tokio task to read from ringbuffer and write to the named pipe
    let runtime_is_running = is_running.clone();
    tokio::spawn(async move {
        // Wait for ffmpeg to connect to the pipe
        log::info!("Waiting for FFmpeg to connect to pipe: {}", pipe_path);
        if let Err(e) = server.connect().await {
            error!("FFmpeg did not connect to pipe: {}", e);
            return;
        }
        info!("FFmpeg connected to pipe: {}", pipe_path);

        let mut buf = vec![0u8; 8192];
        let mut total_written = 0;
        let mut last_log_time = std::time::Instant::now();
        
        while runtime_is_running.load(Ordering::SeqCst) {
            let read_len = cons.pop_slice(&mut buf);
            if read_len > 0 {
                if let Err(e) = server.write_all(&buf[..read_len]).await {
                    error!("Pipe write error: {}", e);
                    break;
                }
                total_written += read_len;
                if last_log_time.elapsed().as_secs() >= 5 {
                    log::info!("Wrote {} bytes to pipe {}", total_written, pipe_path);
                    total_written = 0;
                    last_log_time = std::time::Instant::now();
                }
            } else {
                tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
            }
        }
        log::info!("Pipe stream task ended for {}", pipe_path);
        let _ = server.disconnect();
    });

    let err_fn = |err| error!("An error occurred on the audio stream: {}", err);

    let stream = match config.sample_format() {
        SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &_| write_to_ringbuf(&mut prod, data),
            err_fn,
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            &config.into(),
            move |data: &[i16], _: &_| {
                let f32_data: Vec<f32> = data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                write_to_ringbuf(&mut prod, &f32_data);
            },
            err_fn,
            None,
        ),
        SampleFormat::U16 => device.build_input_stream(
            &config.into(),
            move |data: &[u16], _: &_| {
                let f32_data: Vec<f32> = data
                    .iter()
                    .map(|&s| (s as f32 - u16::MAX as f32 / 2.0) / (u16::MAX as f32 / 2.0))
                    .collect();
                write_to_ringbuf(&mut prod, &f32_data);
            },
            err_fn,
            None,
        ),
        _ => return Err("Unsupported sample format".to_string()),
    }
    .map_err(|e| format!("Failed to build stream: {}", e))?;

    stream
        .play()
        .map_err(|e| format!("Failed to play stream: {}", e))?;

    Ok((stream, pipe_name, sample_rate, channels))
}

fn write_to_ringbuf(prod: &mut impl Producer<Item = u8>, data: &[f32]) {
    // Convert &[f32] to &[u8]
    let bytes: &[u8] = unsafe {
        std::slice::from_raw_parts(
            data.as_ptr() as *const u8,
            data.len() * std::mem::size_of::<f32>(),
        )
    };
    let _ = prod.push_slice(bytes);
}
