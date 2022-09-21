use clap::Parser;
use futuresdr::futures::channel::mpsc;
use futuresdr::futures::StreamExt;

use futuresdr::anyhow::Result;
use futuresdr::async_io::block_on;
use futuresdr::blocks::Apply;
use futuresdr::blocks::Combine;
use futuresdr::blocks::Fft;
use futuresdr::blocks::FftDirection;
use futuresdr::blocks::MessagePipe;
use futuresdr::blocks::SoapySinkBuilder;
use futuresdr::blocks::SoapySourceBuilder;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::buffer::circular::Circular;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;

use wlan::fft_tag_propagation;
use wlan::parse_channel;
use wlan::Decoder;
use wlan::Delay;
use wlan::Encoder;
use wlan::FrameEqualizer;
use wlan::Mac;
use wlan::Mapper;
use wlan::Mcs;
use wlan::MovingAverage;
use wlan::Prefix;
use wlan::SyncLong;
use wlan::SyncShort;
use wlan::MAX_SYM;

const PAD_FRONT: usize = 10000;
const PAD_TAIL: usize = 10000;

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    /// RX Antenna
    #[clap(long)]
    rx_antenna: Option<String>,
    /// RX Soapy Filter
    #[clap(long)]
    rx_filter: Option<String>,
    /// RX Gain
    #[clap(long, default_value_t = 60.0)]
    rx_gain: f64,
    /// TX Antenna
    #[clap(long)]
    tx_antenna: Option<String>,
    /// TX Soapy Filter
    #[clap(long)]
    tx_filter: Option<String>,
    /// TX Gain
    #[clap(long, default_value_t = 60.0)]
    tx_gain: f64,
    /// Sample Rate
    #[clap(long, default_value_t = 20e6)]
    sample_rate: f64,
    /// WLAN RX Channel Number
    #[clap(long, value_parser = parse_channel, default_value = "34")]
    rx_channel: f64,
    /// WLAN TX Channel Number
    #[clap(long, value_parser = parse_channel, default_value = "34")]
    tx_channel: f64,
    /// TX MCS
    #[clap(long, value_parser = Mcs::parse, default_value = "qpsk12")]
    mcs: Mcs,
    /// local UDP port to receive messages to send
    #[clap(long, value_parser)]
    local_port: Option<u32>,
    /// remote UDP server to forward received messages to
    #[clap(long, value_parser)]
    remote_udp: Option<String>,
    /// send periodic messages for testing
    #[clap(long, value_parser)]
    tx_interval: Option<f32>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    println!("Configuration: {:?}", args);

    let mut size = 4096;
    let prefix_in_size = loop {
        if size / 8 >= MAX_SYM * 64 {
            break size;
        }
        size += 4096
    };
    let mut size = 4096;
    let prefix_out_size = loop {
        if size / 8 >= PAD_FRONT + std::cmp::max(PAD_TAIL, 1) + 320 + MAX_SYM * 80 {
            break size;
        }
        size += 4096
    };

    let mut fg = Flowgraph::new();

    // ============================================
    // RECEIVER
    // ============================================
    let mut soapy = SoapySourceBuilder::new()
        .freq(args.rx_channel)
        .sample_rate(args.sample_rate)
        .gain(args.rx_gain);
    if let Some(a) = args.rx_antenna {
        soapy = soapy.antenna(a);
    }
    if let Some(f) = args.rx_filter {
        soapy = soapy.filter(f);
    }
    let src = fg.add_block(soapy.build());
    let delay = fg.add_block(Delay::<Complex32>::new(16));
    fg.connect_stream(src, "out", delay, "in")?;

    let complex_to_mag_2 = fg.add_block(Apply::new(|i: &Complex32| i.norm_sqr()));
    let float_avg = fg.add_block(MovingAverage::<f32>::new(64));
    fg.connect_stream(src, "out", complex_to_mag_2, "in")?;
    fg.connect_stream(complex_to_mag_2, "out", float_avg, "in")?;

    let mult_conj = fg.add_block(Combine::new(|a: &Complex32, b: &Complex32| a * b.conj()));
    let complex_avg = fg.add_block(MovingAverage::<Complex32>::new(48));
    fg.connect_stream(src, "out", mult_conj, "in0")?;
    fg.connect_stream(delay, "out", mult_conj, "in1")?;
    fg.connect_stream(mult_conj, "out", complex_avg, "in")?;

    let divide_mag = fg.add_block(Combine::new(|a: &Complex32, b: &f32| a.norm() / b));
    fg.connect_stream(complex_avg, "out", divide_mag, "in0")?;
    fg.connect_stream(float_avg, "out", divide_mag, "in1")?;

    let sync_short = fg.add_block(SyncShort::new());
    fg.connect_stream(delay, "out", sync_short, "in_sig")?;
    fg.connect_stream(complex_avg, "out", sync_short, "in_abs")?;
    fg.connect_stream(divide_mag, "out", sync_short, "in_cor")?;

    let sync_long = fg.add_block(SyncLong::new());
    fg.connect_stream(sync_short, "out", sync_long, "in")?;

    let mut fft = Fft::new(64);
    fft.set_tag_propagation(Box::new(fft_tag_propagation));
    let fft = fg.add_block(fft);
    fg.connect_stream(sync_long, "out", fft, "in")?;

    let frame_equalizer = fg.add_block(FrameEqualizer::new());
    fg.connect_stream(fft, "out", frame_equalizer, "in")?;

    let decoder = fg.add_block(Decoder::new());
    fg.connect_stream(frame_equalizer, "out", decoder, "in")?;

    let (tx_frame, mut rx_frame) = mpsc::channel::<Pmt>(100);
    let message_pipe = fg.add_block(MessagePipe::new(tx_frame));
    fg.connect_message(decoder, "rx_frames", message_pipe, "in")?;

    // ============================================
    // TRANSMITTER
    // ============================================
    let mac = fg.add_block(Mac::new([0x42; 6], [0x23; 6], [0xff; 6]));
    let encoder = fg.add_block(Encoder::new(Mcs::Qpsk_1_2));
    fg.connect_message(mac, "tx", encoder, "tx")?;
    let mapper = fg.add_block(Mapper::new());
    fg.connect_stream(encoder, "out", mapper, "in")?;
    let mut fft = Fft::with_options(
        64,
        FftDirection::Inverse,
        true,
        Some((1.0f32 / 52.0).sqrt()),
    );
    fft.set_tag_propagation(Box::new(fft_tag_propagation));
    let fft = fg.add_block(fft);
    fg.connect_stream(mapper, "out", fft, "in")?;
    let prefix = fg.add_block(Prefix::new(PAD_FRONT, PAD_TAIL));
    fg.connect_stream_with_type(
        fft,
        "out",
        prefix,
        "in",
        Circular::with_size(prefix_in_size),
    )?;
    let mut soapy = SoapySinkBuilder::new()
        .freq(args.tx_channel)
        .sample_rate(args.sample_rate)
        .gain(args.tx_gain);
    if let Some(a) = args.tx_antenna {
        soapy = soapy.antenna(a);
    }
    if let Some(f) = args.tx_filter {
        soapy = soapy.filter(f);
    }
    let soapy_snk = fg.add_block(soapy.build());
    fg.connect_stream_with_type(
        prefix,
        "out",
        soapy_snk,
        "in",
        Circular::with_size(prefix_out_size),
    )?;

    let rt = Runtime::new();
    let (_fg, _handle) = block_on(rt.start(fg));

    rt.block_on(async move {
        while let Some(x) = rx_frame.next().await {
            match x {
                Pmt::Blob(data) => {
                    println!("received frame ({:?} bytes)", data.len());
                }
                _ => break,
            }
        }
    });

    Ok(())
}
