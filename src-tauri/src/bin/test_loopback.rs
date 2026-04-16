use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

fn main() {
    let host = cpal::default_host();
    let device = host.default_output_device().unwrap();
    let config = device.default_output_config().unwrap();
    
    // see if build_input_stream_loopback compiles
    let stream = device.build_input_stream_loopback(
        &config.into(),
        |data: &[f32], _: &_| {},
        |err| eprintln!("err: {}", err),
        None,
    );
}
