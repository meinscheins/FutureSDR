use clap::Parser;
use std::time::Duration;

use futuresdr::anyhow::Result;
use futuresdr::async_io::block_on;
use futuresdr::async_io::Timer;
use futuresdr::async_net::SocketAddr;
use futuresdr::async_net::UdpSocket;
use futuresdr::blocks::Apply;
use futuresdr::blocks::Combine;
use futuresdr::blocks::Fft;
use futuresdr::blocks::FftDirection;
use futuresdr::blocks::MessagePipe;
use futuresdr::blocks::SoapySinkBuilder;
use futuresdr::blocks::SoapySourceBuilder;
use futuresdr::blocks::WebsocketSinkBuilder;
use futuresdr::blocks::WebsocketSinkMode;
use futuresdr::futures::channel::mpsc;
use futuresdr::futures::channel::oneshot;
use futuresdr::futures::StreamExt;
use futuresdr::log::info;
use futuresdr::log::warn;
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
use wlan::FftShift;
use wlan::FrameEqualizer;
use wlan::Keep1InN;
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
    /// Stream Spectrum data at ws://0.0.0.0:9001
    #[clap(long, value_parser)]
    spectrum: bool,
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
    // TRANSMITTER
    // ============================================
    let mac = fg.add_block(Mac::new([0x42; 6], [0x23; 6], [0xff; 6]));
    let encoder = fg.add_block(Encoder::new(args.mcs));
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

    let (rxed_sender, mut rxed_frames) = mpsc::channel::<Pmt>(100);
    let message_pipe = fg.add_block(MessagePipe::new(rxed_sender));
    fg.connect_message(decoder, "rx_frames", message_pipe, "in")?;

    // ========================================
    // Spectrum
    // ========================================
    if args.spectrum {
        let snk = fg.add_block(
            WebsocketSinkBuilder::<f32>::new(9001)
                .mode(WebsocketSinkMode::FixedDropping(2048))
                .build(),
        );
        let fft = fg.add_block(Fft::new(2048));
        let shift = fg.add_block(FftShift::new());
        let keep = fg.add_block(Keep1InN::new(0.5, 10));
        let cpy = fg.add_block(futuresdr::blocks::Copy::<Complex32>::new());

        fg.connect_stream(src, "out", cpy, "in")?;
        fg.connect_stream(cpy, "out", fft, "in")?;
        fg.connect_stream(fft, "out", shift, "in")?;
        fg.connect_stream(shift, "out", keep, "in")?;
        fg.connect_stream(keep, "out", snk, "in")?;
    }

    let rt = Runtime::new();
    let (fg, mut handle) = block_on(rt.start(fg));

    // if tx_interval is set, send messages periodically
    if let Some(tx_interval) = args.tx_interval {
        let mut seq = 0u64;
        let mut myhandle = handle.clone();
        rt.spawn_background(async move {
            loop {
                Timer::after(Duration::from_secs_f32(tx_interval)).await;
                myhandle
                    .call(
                        mac,
                        0,
                        Pmt::Blob(format!("FutureSDR {}", seq).as_bytes().to_vec()),
                    )
                    .await
                    .unwrap();
                seq += 1;
            }
        });
    }

    // we are the udp server
    if let Some(port) = args.local_port {
        info!("Acting as UDP server.");
        let (tx_endpoint, rx_endpoint) = oneshot::channel::<SocketAddr>();
        let socket = block_on(UdpSocket::bind(format!("0.0.0.0:{}", port))).unwrap();
        let socket2 = socket.clone();

        rt.spawn_background(async move {
            let mut buf = vec![0u8; 1024];

            let (n, e) = socket.recv_from(&mut buf).await.unwrap();
            handle
                .call(mac, 0, Pmt::Blob(buf[0..n].to_vec()))
                .await
                .unwrap();

            tx_endpoint.send(e).unwrap();

            loop {
                let (n, _) = socket.recv_from(&mut buf).await.unwrap();
                handle
                    .call(mac, 0, Pmt::Blob(buf[0..n].to_vec()))
                    .await
                    .unwrap();
            }
        });

        rt.spawn_background(async move {
            let endpoint = rx_endpoint.await.unwrap();
            info!("endpoint connected to local udp server {:?}", &endpoint);

            loop {
                if let Some(p) = rxed_frames.next().await {
                    if let Pmt::Blob(v) = p {
                        socket2.send_to(&v, endpoint).await.unwrap();
                    } else {
                        warn!("pmt to tx was not a blob");
                    }
                } else {
                    warn!("cannot read from MessagePipe receiver");
                }
            }
        });
    } else if let Some(remote) = args.remote_udp {
        info!("Acting as UDP client.");
        let socket = block_on(UdpSocket::bind(format!("0.0.0.0:{}", 0))).unwrap();
        block_on(socket.connect(remote)).unwrap();
        let socket2 = socket.clone();

        rt.spawn_background(async move {
            let mut buf = vec![0u8; 1024];
            loop {
                let (n, _) = socket.recv_from(&mut buf).await.unwrap();
                handle
                    .call(mac, 0, Pmt::Blob(buf[0..n].to_vec()))
                    .await
                    .unwrap();
            }
        });

        rt.spawn_background(async move {
            loop {
                if let Some(p) = rxed_frames.next().await {
                    if let Pmt::Blob(v) = p {
                        socket2.send(&v).await.unwrap();
                    } else {
                        warn!("pmt to tx was not a blob");
                    }
                } else {
                    warn!("cannot read from MessagePipe receiver");
                }
            }
        });
    } else {
        info!("No UDP forwarding configured");
        rt.spawn_background(async move {
            loop {
                if let Some(_p) = rxed_frames.next().await {
                    info!("FRAAAAAAAAME");
                } else {
                    warn!("cannot read from MessagePipe receiver");
                }
            }
        });
    }

    block_on(fg)?;

    Ok(())
}
