use crate::scream::{ScreamHeader, ScreamHeaderArray};
use crate::Args;
use cpal::traits::{DeviceTrait, StreamTrait};
use ringbuf::RingBuffer;

const MAX_CHANNELS: usize = 10;

#[derive(Debug, Clone)]
struct NoSamplesInBufferError;

#[derive(PartialEq, Debug, Clone, Copy)]
enum OutputMode {
    Stopped,
    ChuggingAlong,
    PlaySlower,
    PlayFaster,
}

pub type BufferSample = [f32; MAX_CHANNELS];

pub struct AudioPlayer {
    pub buffer: ringbuf::Producer<BufferSample>,
    #[allow(dead_code)]
    stream: cpal::Stream,
}

pub fn create_audio_player(
    device: &cpal::Device,
    header: &ScreamHeaderArray,
    args: &Args,
) -> anyhow::Result<AudioPlayer> {
    let buf = RingBuffer::<BufferSample>::new(args.samples_buffered * 10);
    let (prod, cons) = buf.split();

    let stream_config = cpal::StreamConfig {
        buffer_size: cpal::BufferSize::Default,
        channels: header.channels(),
        sample_rate: cpal::SampleRate(header.sample_rate()),
    };

    let stream = match device.default_output_config()?.sample_format() {
        cpal::SampleFormat::F32 => {
            build_output_stream::<f32>(&device, &stream_config, cons, args.clone())
        }
        cpal::SampleFormat::I16 => {
            build_output_stream::<i16>(&device, &stream_config, cons, args.clone())
        }
        cpal::SampleFormat::U16 => {
            build_output_stream::<u16>(&device, &stream_config, cons, args.clone())
        }
    }?;

    stream.play()?;

    Ok(AudioPlayer {
        stream: stream,
        buffer: prod,
    })
}

fn get_output_mode(
    current_output_mode: OutputMode,
    samples_requested: usize,
    samples_available: usize,
    args: &Args,
) -> OutputMode {
    if samples_available == 0 {
        return OutputMode::Stopped;
    }

    if current_output_mode == OutputMode::Stopped && samples_available > samples_requested {
        return OutputMode::ChuggingAlong;
    }

    if samples_available < (samples_requested as f32 * args.slower_playback_threshold) as usize {
        return OutputMode::PlaySlower;
    }

    if samples_available > (samples_requested as f32 * args.faster_playback_threshold) as usize {
        return OutputMode::PlayFaster;
    }

    let back_to_chug_low = (samples_requested as f32 / args.normal_playback_threshold) as usize;
    let back_to_chug_high = (samples_requested as f32 * args.normal_playback_threshold) as usize;
    if back_to_chug_low < samples_available && samples_available < back_to_chug_high {
        return OutputMode::ChuggingAlong;
    }

    return current_output_mode;
}

fn get_sample(
    output_mode: OutputMode,
    cons: &mut ringbuf::Consumer<BufferSample>,
    last_sample: &BufferSample,
    iteration: i32,
) -> Result<[f32; 10], NoSamplesInBufferError> {
    match output_mode {
        OutputMode::Stopped => Ok(*last_sample),
        OutputMode::ChuggingAlong => cons.pop().ok_or(NoSamplesInBufferError),
        OutputMode::PlayFaster => {
            // pop an extra one
            cons.pop().ok_or(NoSamplesInBufferError)?;
            cons.pop().ok_or(NoSamplesInBufferError)
        }
        OutputMode::PlaySlower => {
            // half of the time, return the previous sample instead
            if iteration % 2 == 0 {
                Ok(*last_sample)
            } else {
                cons.pop().ok_or(NoSamplesInBufferError)
            }
        }
    }
}

fn build_output_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut cons: ringbuf::Consumer<BufferSample>,
    args: Args,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: cpal::Sample,
{
    let channels = config.channels as usize;
    let mut iteration: i32 = 0;
    let mut output_mode = OutputMode::Stopped;
    let mut last_sample: BufferSample = [0.0; MAX_CHANNELS];

    device.build_output_stream(
        &config,
        move |output: &mut [T], _: &cpal::OutputCallbackInfo| {
            let samples_requested = output.len() / channels;
            let necessary_buffer_size = std::cmp::max(args.samples_buffered, samples_requested);

            for frame in output.chunks_mut(channels.into()) {
                iteration += 1;

                let new_output_mode =
                    get_output_mode(output_mode, necessary_buffer_size, cons.len(), &args);

                if output_mode != new_output_mode {
                    println!(
                        "Output mode changed: {:?}, samples: {}, buffer_size: {}",
                        new_output_mode,
                        cons.len(),
                        necessary_buffer_size
                    );
                }

                output_mode = new_output_mode;

                let sample = get_sample(output_mode, &mut cons, &last_sample, iteration)
                    .unwrap_or(last_sample);

                for (channel, channel_sample) in frame.iter_mut().enumerate() {
                    *channel_sample = cpal::Sample::from(&sample[channel]);
                }

                last_sample = sample;
            }
        },
        |_err| println!("Some weird error huh!"),
    )
}
