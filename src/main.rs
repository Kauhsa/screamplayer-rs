#[macro_use]
extern crate arrayref;

mod output_stream;
mod scream;

use byteorder::{ByteOrder, LittleEndian};
use cpal::traits::{DeviceTrait, HostTrait};
use output_stream::{create_audio_player, AudioPlayer};
use scream::{ScreamHeader, ScreamHeaderArray};
use std::io::{Error, ErrorKind};
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::time::Duration;

const SCREAM_PACKET_MAX_SIZE: usize = 1157;

type ScreamPacket = [u8; SCREAM_PACKET_MAX_SIZE];

const ADDR_ANY: Ipv4Addr = Ipv4Addr::new(0, 0, 0, 0);
const SCREAM_MULTICAST_ADDR: Ipv4Addr = Ipv4Addr::new(239, 255, 77, 77);
const SCREAM_MULTICAST_PORT: u16 = 4010;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device = select_cpal_device(Some("Speakers (HyperX Cloud Flight Wireless Headset)"))?;
    // let device = select_cpal_device(None)?;

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
            println!("Output received, starting audio");
            previous_header = *header;
            audio_player_buffer = Box::new(Some(create_audio_player(&device, header)?));
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

fn convert_to_f32_sample<const FROM_SIGNED_BIT_INT: isize>(i: f64) -> f32 {
    if i < 0.0 {
        (i / (2.0f64.powf(FROM_SIGNED_BIT_INT as f64 - 1.0))) as f32
    } else {
        (i / (2.0f64.powf(FROM_SIGNED_BIT_INT as f64 - 1.0) - 1.0)) as f32
    }
}

pub fn convert_to_sample(header: &impl ScreamHeader, sample: &[u8]) -> [f32; 10] {
    let mut new_buf = [0.0f32; 10];

    for (i, channel_sample) in sample.chunks(header.sample_bytes()).enumerate() {
        new_buf[i] = match header.sample_bits() {
            16 => convert_to_f32_sample::<16>(LittleEndian::read_i16(channel_sample).into()),
            24 => convert_to_f32_sample::<24>(LittleEndian::read_i24(channel_sample).into()),
            32 => convert_to_f32_sample::<32>(LittleEndian::read_i32(channel_sample).into()),
            _ => 0.0,
        };
    }

    new_buf
}

fn output_devices(host: cpal::Host) -> Result<Vec<cpal::Device>, cpal::DevicesError> {
    let devices = host
        .devices()?
        .filter(|d| {
            // only devices that support output configurations.
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
