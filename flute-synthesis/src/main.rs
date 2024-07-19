//! Make some noise via cpal.
#![allow(clippy::precedence)]

use assert_no_alloc::*;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use fundsp::hacker::*;

#[cfg(debug_assertions)] // required when disable_release is set (default)
#[global_allocator]
static A: AllocDisabler = AllocDisabler;

fn main() {
    let host = cpal::default_host();

    let device = host
        .default_output_device()
        .expect("Failed to find a default output device");
    let config = device.default_output_config().unwrap();

    match config.sample_format() {
        cpal::SampleFormat::F32 => run::<f32>(&device, &config.into()).unwrap(),
        cpal::SampleFormat::I16 => run::<i16>(&device, &config.into()).unwrap(),
        cpal::SampleFormat::U16 => run::<u16>(&device, &config.into()).unwrap(),
        _ => panic!("Unsupported format"),
    }
}

use fundsp::hacker::*;
use std::f64::consts::PI;
use hound::{WavWriter, WavSpec};
use std::sync::Arc;

// Define basic signal processing functions
fn sine_wave(frequency: f64, amplitude: f64) -> impl AudioNode {
    sine_hz(frequency) * amplitude
}

fn harmonics(fundamental: f64, amplitudes: Vec<(f64, f64)>) -> impl AudioNode {
    let mut nodes: Vec<Arc<dyn AudioUnit64>> = vec![];
    for (ratio, amplitude) in amplitudes {
        nodes.push(Arc::new(sine_wave(fundamental * ratio, amplitude)));
    }
    stack(nodes)
}

fn noise(amplitude: f64) -> impl AudioNode {
    white() * amplitude
}

fn lowpass(a: f64, node: impl AudioNode) -> impl AudioNode {
    lowpass_hz(a) & node
}

fn envelope(a: f64, d: f64, s: f64, r: f64, s_time: f64) -> impl AudioNode {
    // Piecewise linear envelope
    envelope3(a, d, s, r, s_time)
}

// Define flute synthesis function
fn flute(note: f64, length: f64, velocity: f64) -> impl AudioNode {
    // Define the amplitude and frequency characteristics of the flute sound
    let mut node = harmonics(note, vec![
        (1.0, 0.8 * velocity),
        (2.0, 0.4 * velocity),
        (3.0, 0.3 * velocity),
        (4.0, 0.2 * velocity),
        (5.0, 0.1 * velocity),
    ]);

    // Add a noise component
    node = node + noise(0.1 * velocity);

    // Apply an envelope
    node * envelope(0.01, 0.1, 0.7, 0.05, length)
}

fn main() {
    // Example usage: Generate a 2.5 second note
    let sample_rate = 44100;
    let length = 2.5;
    let mut writer = WavWriter::create("output.wav", WavSpec {
        channels: 1,
        sample_rate: sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    }).unwrap();

    let synth = flute(440.0, length, 1.0); // A4 note

    for t in 0..(length * sample_rate as f64) as usize {
        let sample = synth.process64(t as f64 / sample_rate as f64);
        writer.write_sample((sample * i16::MAX as f64) as i16).unwrap();
    }
}


fn run<T>(device: &cpal::Device, config: &cpal::StreamConfig) -> Result<(), anyhow::Error>
where
    T: SizedSample + FromSample<f64>,
{
    let sample_rate = config.sample_rate.0 as f64;
    let channels = config.channels as usize;


    let c = flute(440.0, 5.0, 1.0); // A4 note
   
    let c = c >> pan(0.0);
    let mut c = c
        >> (declick() | declick())
        >> (dcblock() | dcblock())
        >> limiter_stereo(1.0, 5.0);

    c.set_sample_rate(sample_rate);
    c.allocate();

    let mut next_value = move || assert_no_alloc(|| c.get_stereo());

    let err_fn = |err| eprintln!("an error occurred on stream: {}", err);

    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            write_data(data, channels, &mut next_value)
        },
        err_fn,
        None,
    )?;
    stream.play()?;

    std::thread::sleep(std::time::Duration::from_millis(50000));

    Ok(())
}

fn write_data<T>(output: &mut [T], channels: usize, next_sample: &mut dyn FnMut() -> (f32, f32))
where
    T: SizedSample + FromSample<f64>,
{
    for frame in output.chunks_mut(channels) {
        let sample = next_sample();
        let left = T::from_sample(sample.0 as f64);
        let right: T = T::from_sample(sample.1 as f64);

        for (channel, sample) in frame.iter_mut().enumerate() {
            if channel & 1 == 0 {
                *sample = left;
            } else {
                *sample = right;
            }
        }
    }
}
