use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

fn main() {
    let host = cpal::default_host();
    let device = host.default_output_device().expect("no output device");
    let config = device.default_output_config().unwrap();
    println!("Device: {}", device.name().unwrap());
    
    let counter = Arc::new(AtomicUsize::new(0));
    let c2 = counter.clone();

    let stream = device.build_input_stream(
        &config.into(),
        move |data: &[f32], _: &_| {
            c2.fetch_add(data.len(), Ordering::SeqCst);
        },
        |err| eprintln!("err: {}", err),
        None,
    );
    match stream {
        Ok(s) => {
            println!("Stream created successfully.");
            s.play().unwrap();
            std::thread::sleep(std::time::Duration::from_secs(3));
            println!("Received {} samples", counter.load(Ordering::SeqCst));
        }
        Err(e) => {
            eprintln!("Failed to create stream: {}", e);
        }
    }
}
