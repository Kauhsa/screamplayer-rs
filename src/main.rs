#[macro_use]
extern crate arrayref;

use anyhow;
use clap::Parser;

mod client;
mod output_stream;
mod scream;

#[derive(Parser, Debug, Clone)]
#[clap(author, version, about, long_about = None)]
pub struct Args {
    #[clap(short, long, value_parser, default_value_t = 2048)]
    samples_buffered: usize,

    #[clap(long, value_parser, default_value_t = 1.1)]
    normal_playback_threshold: f32,

    #[clap(long, value_parser, default_value_t = 0.5)]
    slower_playback_threshold: f32,

    #[clap(long, value_parser, default_value_t = 2.0)]
    faster_playback_threshold: f32,

    #[clap(short, long, value_parser)]
    output_device: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    client::start_client(&args)
}
