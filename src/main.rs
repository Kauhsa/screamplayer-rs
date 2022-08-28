#[macro_use]
extern crate arrayref;

mod scream;

use byteorder::{ByteOrder, LittleEndian};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::RingBuffer;
use scream::{ScreamHeader, ScreamHeaderArray};
use std::io::{Error, ErrorKind};
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::time::Duration;

const SCREAM_PACKET_MAX_SIZE: usize = 1157;
const NETWORK_BUFFER_SIZE: usize = 2048;
const MAX_CHANNELS: usize = 10;

type ScreamPacket = [u8; SCREAM_PACKET_MAX_SIZE];
type BufferSample = [i16; MAX_CHANNELS];

struct AudioPlayer {
    buffer: ringbuf::Producer<BufferSample>,
    stream: cpal::Stream,
}

const ADDR_ANY: Ipv4Addr = Ipv4Addr::new(0, 0, 0, 0);
const SCREAM_MULTICAST_ADDR: Ipv4Addr = Ipv4Addr::new(239, 255, 77, 77);
const SCREAM_MULTICAST_PORT: u16 = 4010;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device = select_cpal_device(Some("Speakers (HyperX Cloud Flight Wireless Headset)"))?;

    let socket = UdpSocket::bind(SocketAddrV4::new(ADDR_ANY, SCREAM_MULTICAST_PORT))?;
    socket.join_multicast_v4(&SCREAM_MULTICAST_ADDR, &ADDR_ANY)?;
    socket.set_read_timeout(Some(Duration::new(1, 0)))?;

    let mut audio_player_buffer: Box<Option<AudioPlayer>> = Box::new(None);
    let mut buf: ScreamPacket = [0u8; SCREAM_PACKET_MAX_SIZE];
    let mut previous_header: ScreamHeaderArray = [0u8; 5];

    loop {
        let res = socket.recv_from(&mut buf);

        match &res {
            Err(e) => {
                if e.kind() == ErrorKind::TimedOut {
                    if (&audio_player_buffer).is_some() {
                        println!("No output, stopping audio.");
                        audio_player_buffer = Box::new(None);
                    }
                    continue;
                }
            }

            _ => (),
        }

        let (size, _addr) = res?;
        let header: &ScreamHeaderArray = array_ref![buf, 0, 5];
        let samples = &buf[5..size];

        if (&audio_player_buffer).is_none() || previous_header.as_slice() != header.as_slice() {
            previous_header = *header;
            audio_player_buffer = Box::new(Some(create_audio_player(&device, header)?))
        }

        let current_audio_player = audio_player_buffer.as_mut().as_mut().unwrap();

        let packet_sample_bytes =
            samples.chunks_exact(header.sample_bytes() * header.channels() as usize);

        for sample_bytes in packet_sample_bytes {
            let buffer_sample = convert_to_sample(header, sample_bytes);

            match current_audio_player.buffer.push(buffer_sample) {
                Err(_err) => println!("Buffer overflow"),
                _ => (),
            }
        }
    }
}

pub fn convert_to_sample(header: &impl ScreamHeader, sample: &[u8]) -> [i16; 10] {
    let mut new_buf = [0i16; 10];

    for (i, channel_sample) in sample.chunks(header.sample_bytes()).enumerate() {
        new_buf[i] = match header.sample_bits() {
            16 => LittleEndian::read_i16(channel_sample) as i16,
            24 => LittleEndian::read_i24(channel_sample) as i16, // TODO.
            32 => LittleEndian::read_i32(channel_sample) as i16, // TODO.
            _ => 0,
        };
    }

    new_buf
}

fn output_devices(host: cpal::Host) -> Result<Vec<cpal::Device>, cpal::DevicesError> {
    let devices = host
        .devices()?
        .filter(|d| {
            // only devices that support configurations.
            d.supported_output_configs()
                .map(|mut x| x.next() != None)
                .unwrap_or(false)
        })
        .collect();

    Ok(devices)
}

fn select_cpal_device(name: Option<&str>) -> Result<cpal::Device, Box<dyn std::error::Error>> {
    let host = cpal::default_host();

    let device = match name {
        Some(n) => output_devices(host)?
            .into_iter()
            .find(|d| d.name().map(|name| name == n).unwrap_or(false)),
        None => host.default_output_device(),
    };

    device.ok_or(Box::new(Error::new(
        ErrorKind::NotFound,
        "Could not find audio device",
    )))
}

fn create_audio_player(
    device: &cpal::Device,
    header: &ScreamHeaderArray,
) -> Result<AudioPlayer, Box<dyn std::error::Error>> {
    let buf = RingBuffer::<BufferSample>::new(1024 * 10);
    let (prod, cons) = buf.split();

    let stream_config = cpal::StreamConfig {
        buffer_size: cpal::BufferSize::Default,
        channels: header.channels(),
        sample_rate: cpal::SampleRate(header.sample_rate()),
    };

    let stream = match device.default_output_config()?.sample_format() {
        cpal::SampleFormat::F32 => build_output_stream::<f32>(&device, &stream_config, cons),
        cpal::SampleFormat::I16 => build_output_stream::<i16>(&device, &stream_config, cons),
        cpal::SampleFormat::U16 => build_output_stream::<u16>(&device, &stream_config, cons),
    }?;

    stream.play()?;

    Ok(AudioPlayer {
        stream: stream,
        buffer: prod,
    })
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

    device.build_output_stream(
        &config,
        move |output: &mut [T], _: &cpal::OutputCallbackInfo| {
            let necessary_buffer_size = std::cmp::max(NETWORK_BUFFER_SIZE, output.len() / channels);

            if cons.len() < necessary_buffer_size {
                return;
            }

            if cons.len() > necessary_buffer_size * 5 {
                println!("Buffer getting overrun, pop some out...");
                cons.discard(necessary_buffer_size * 4);
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
    )
}
