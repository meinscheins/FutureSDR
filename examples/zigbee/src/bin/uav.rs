use clap::Parser;
use futures::channel::mpsc;
use futures::channel::oneshot;
use futures::StreamExt;
use std::time::Duration;

use futuresdr::anyhow::Result;
use futuresdr::async_io::block_on;
use futuresdr::async_io::Timer;
use futuresdr::async_net::SocketAddr;
use futuresdr::async_net::UdpSocket;
use futuresdr::blocks::Apply;
use futuresdr::blocks::MessagePipe;
use futuresdr::blocks::SoapySinkBuilder;
use futuresdr::blocks::SoapySourceBuilder;
use futuresdr::log::info;
use futuresdr::log::warn;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;

use zigbee::channel_to_freq;
use zigbee::modulator;
use zigbee::ClockRecoveryMm;
use zigbee::Decoder;
use zigbee::IqDelay;
use zigbee::Mac;

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    /// ZigBee channel for receiver (11..=26)
    #[clap(long, default_value_t = 26)]
    rx_channel: u32,
    /// ZigBee channel for transmitter (11..=26)
    #[clap(long, default_value_t = 26)]
    tx_channel: u32,
    #[clap(long, default_value_t = 20.0)]
    rx_gain: f64,
    #[clap(long, default_value_t = 20.0)]
    tx_gain: f64,
    #[clap(long)]
    tx_interval: Option<f32>,
    /// Message that is sent if a tx-interval is set
    #[clap(long, default_value = "FutureSDR")]
    tx_msg: String,
    #[clap(long, default_value = "TX/RX")]
    tx_antenna: String,
    #[clap(long, default_value = "RX2")]
    rx_antenna: String,
    /// X310 IP
    #[clap(long, default_value = "10.10.23.1")]
    sdr_ip: String,
    /// local UDP port to receive messages to send
    #[clap(long)]
    local_port: Option<u32>,
    /// remote UDP server to forward received messages to
    #[clap(long)]
    remote_udp: Option<String>,
}

fn main() -> Result<()> {
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );
    let args = Args::parse();
    if args.local_port.is_some() && args.remote_udp.is_some() {
        eprintln!("local port and remote udp should not both be set");
        std::process::exit(1);
    }
    let tx_freq = channel_to_freq(args.tx_channel)?;
    let rx_freq = channel_to_freq(args.rx_channel)?;
    info!("Configuration: {:?}", args);

    let mut fg = Flowgraph::new();

    // ========================================
    // TRANSMITTER
    // ========================================
    let mac = fg.add_block(Mac::new());
    let modulator = fg.add_block(modulator());
    let iq_delay = fg.add_block(IqDelay::new());
    let soapy_snk = fg.add_block(
        SoapySinkBuilder::new()
            .filter(format!("uhd,type=x300,addr={}", args.sdr_ip))
            .antenna(args.tx_antenna)
            .freq(tx_freq)
            .sample_rate(4e6)
            .gain(args.tx_gain)
            .build(),
    );

    fg.connect_stream(mac, "out", modulator, "in")?;
    fg.connect_stream(modulator, "out", iq_delay, "in")?;
    fg.connect_stream(iq_delay, "out", soapy_snk, "in")?;

    // ========================================
    // Receiver
    // ========================================
    let src = fg.add_block(
        SoapySourceBuilder::new()
            .filter(format!("uhd,type=x300,addr={}", args.sdr_ip))
            .freq(rx_freq)
            .antenna(args.rx_antenna)
            .sample_rate(4e6)
            .gain(args.rx_gain)
            .build(),
    );

    let mut last: Complex32 = Complex32::new(0.0, 0.0);
    let mut iir: f32 = 0.0;
    let alpha = 0.00016;
    let avg = fg.add_block(Apply::new(move |i: &Complex32| -> f32 {
        let phase = (last.conj() * i).arg();
        last = *i;
        iir = (1.0 - alpha) * iir + alpha * phase;
        phase - iir
    }));

    let omega = 2.0;
    let gain_omega = 0.000225;
    let mu = 0.5;
    let gain_mu = 0.03;
    let omega_relative_limit = 0.0002;
    let mm = fg.add_block(ClockRecoveryMm::new(
        omega,
        gain_omega,
        mu,
        gain_mu,
        omega_relative_limit,
    ));

    let decoder = fg.add_block(Decoder::new(6));

    fg.connect_stream(src, "out", avg, "in")?;
    fg.connect_stream(avg, "out", mm, "in")?;
    fg.connect_stream(mm, "out", decoder, "in")?;
    fg.connect_message(decoder, "out", mac, "rx")?;

    let (sender, mut receiver) = mpsc::channel::<Pmt>(10);
    let pipe = fg.add_block(MessagePipe::new(sender));
    fg.connect_message(mac, "rxed", pipe, "in")?;

    let rt = Runtime::new();
    let (fg, mut handle) = rt.start(fg);
    let mut handle2 = handle.clone();

    // if tx_interval is set, send messages periodically
    if let Some(tx_interval) = args.tx_interval {
        let mut seq = 0u64;
        rt.spawn_background(async move {
            loop {
                Timer::after(Duration::from_secs_f32(tx_interval)).await;
                handle
                    .call(
                        0, // mac block
                        1, // tx handler
                        Pmt::Blob(format!("{} {}", args.tx_msg, seq).as_bytes().to_vec()),
                    )
                    .await
                    .unwrap();
                seq += 1;
            }
        });
    }

    // we are the udp server
    if let Some(port) = args.local_port {
        let (tx_endpoint, rx_endpoint) = oneshot::channel::<SocketAddr>();
        let socket = block_on(UdpSocket::bind(format!("0.0.0.0:{}", port))).unwrap();
        let socket2 = socket.clone();

        rt.spawn_background(async move {
            let mut buf = vec![0u8; 1024];

            let (n, e) = socket.recv_from(&mut buf).await.unwrap();
            handle2
                .call(
                    0, // mac block
                    1, // tx handler
                    Pmt::Blob(buf[0..n].to_vec()),
                )
                .await
                .unwrap();

            tx_endpoint.send(e).unwrap();

            loop {
                let (n, _) = socket.recv_from(&mut buf).await.unwrap();
                handle2
                    .call(
                        0, // mac block
                        1, // tx handler
                        Pmt::Blob(buf[0..n].to_vec()),
                    )
                    .await
                    .unwrap();
            }
        });

        rt.spawn_background(async move {
            let endpoint = rx_endpoint.await.unwrap();
            info!("endpoint connected to local udp server {:?}", &endpoint);

            loop {
                if let Some(p) = receiver.next().await {
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
        let socket = block_on(UdpSocket::bind(format!("0.0.0.0:{}", 0))).unwrap();
        block_on(socket.connect(remote)).unwrap();
        let socket2 = socket.clone();

        rt.spawn_background(async move {
            let mut buf = vec![0u8; 1024];
            loop {
                let (n, _) = socket.recv_from(&mut buf).await.unwrap();
                handle2
                    .call(
                        0, // mac block
                        1, // tx handler
                        Pmt::Blob(buf[0..n].to_vec()),
                    )
                    .await
                    .unwrap();
            }
        });

        rt.spawn_background(async move {
            loop {
                if let Some(p) = receiver.next().await {
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
    }

    block_on(fg)?;

    Ok(())
}
