use byteorder::{BigEndian, ByteOrder, LittleEndian};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::RingBuffer;
use std::net::{Ipv4Addr, UdpSocket};

const SCREAM_PACKET_MAX_SIZE: usize = 1157;
const SCREAM_PACKET_HEADER_SIZE: usize = 5;
const BUFFER_SIZE: usize = 1024;
const MAX_CHANNELS: usize = 10;

type ScreamPacket = [u8; SCREAM_PACKET_MAX_SIZE];
type BufferSample = [i16; MAX_CHANNELS];

struct AudioPlayer {
    buffer: ringbuf::Producer<BufferSample>,
    stream: cpal::Stream,
}

fn main() -> std::io::Result<()> {
    let inaddr_any = "0.0.0.0"
        .parse::<Ipv4Addr>()
        .expect("Could not parse inaddrAny");

    let scream_addr = "239.255.77.77"
        .parse::<Ipv4Addr>()
        .expect("Could not parse address for scream");

    let socket = UdpSocket::bind("0.0.0.0:4010").expect("Could not bind socket");

    socket
        .join_multicast_v4(&scream_addr, &inaddr_any)
        .expect("Could not join multicast");

    let mut audio_player_buffer: Box<Option<AudioPlayer>> = Box::new(None);
    let mut buf: ScreamPacket = [0u8; SCREAM_PACKET_MAX_SIZE];

    loop {
        let (size, _addr) = socket
            .recv_from(&mut buf)
            .expect("Error while waiting data from socket");

        // println!("Buf {:?}", buf);

        if (&audio_player_buffer).is_none() {
            audio_player_buffer = Box::new(Some(create_audio_player(&buf)))
        }

        let sample_size = buf[1] as usize;
        let sample_size_bytes = sample_size / 8;

        let channels = 2; // TODO.
        let samples =
            (&buf[SCREAM_PACKET_HEADER_SIZE..size]).chunks_exact(sample_size_bytes * channels);

        for sample in samples {
            let mut new_buf: BufferSample = [0i16; MAX_CHANNELS];

            for (i, channel_sample) in sample.chunks(sample_size_bytes).enumerate() {
                new_buf[i] = match sample_size {
                    16 => LittleEndian::read_i16(channel_sample) as i16,
                    24 => LittleEndian::read_i24(channel_sample) as i16, // TODO.
                    32 => LittleEndian::read_i32(channel_sample) as i16, // TODO.
                    _ => 0,
                };
            }

            let current_audio_player = audio_player_buffer
                .as_mut()
                .as_mut()
                .expect("No current audio player");

            match current_audio_player.buffer.push(new_buf) {
                Ok(_) => {} // all ok
                Err(_item) => println!("Buffer full!"),
            }
        }
    }
}

fn parse_sampling_rate(rate: u8) -> u32 {
    let multiplier = (rate & 0b01111111) as u32;

    return match rate & 0b10000000 == 0 {
        true => 48000 * multiplier,
        false => 44100 * multiplier,
    };
}

fn create_audio_player(initial_packet: &ScreamPacket) -> AudioPlayer {
    let buf = RingBuffer::<BufferSample>::new(BUFFER_SIZE);
    let (prod, cons) = buf.split();

    let host = cpal::default_host();
    let mut devices = host.devices().expect("Output devices cannot be listed");

    let device = devices
        .find(|d| {
            d.name().expect("Device has no name?")
                == "Speakers (HyperX Cloud Flight Wireless Headset)"
        })
        .expect("Cannot load default output device");

    let stream_config = cpal::StreamConfig {
        buffer_size: cpal::BufferSize::Default,
        channels: 2, // TODO: hardcoded for now.
        sample_rate: cpal::SampleRate(parse_sampling_rate(initial_packet[0])),
    };

    let stream = match device.default_output_config().unwrap().sample_format() {
        cpal::SampleFormat::F32 => build_output_stream::<f32>(&device, &stream_config, cons),
        cpal::SampleFormat::I16 => build_output_stream::<i16>(&device, &stream_config, cons),
        cpal::SampleFormat::U16 => build_output_stream::<u16>(&device, &stream_config, cons),
    }
    .expect("Could not create stream");

    stream.play().expect("Could not play stream");

    return AudioPlayer {
        stream: stream,
        buffer: prod,
    };
}

fn build_output_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut cons: ringbuf::Consumer<BufferSample>,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: cpal::Sample,
{
    let channels = config.channels as usize;

    return device.build_output_stream(
        &config,
        move |output: &mut [T], _: &cpal::OutputCallbackInfo| {
            if cons.len() < (output.len() / channels) {
                println!("Buffer underrun...");
                return;
            }

            for frame in output.chunks_mut(channels.into()) {
                let sample = cons.pop();

                match sample {
                    Some(s) => {
                        for (channel, channel_sample) in frame.iter_mut().enumerate() {
                            *channel_sample = cpal::Sample::from(&s[channel]);
                        }
                    }

                    None => println!("Buffer underrun!"),
                }
            }
        },
        |_err| println!("Some weird error huh!"),
    );
}
