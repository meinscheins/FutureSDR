use clap::Parser;
use std::sync::mpsc::channel;
use std::time::Duration;

use futuresdr::anyhow::Result;
use futuresdr::async_io;
use futuresdr::async_io::block_on;
use futuresdr::async_io::Timer;
use futuresdr::async_net::SocketAddr;
use futuresdr::async_net::UdpSocket;
use futuresdr::blocks::Apply;
use futuresdr::blocks::Combine;
use futuresdr::blocks::Fft;
use futuresdr::blocks::FftDirection;
use futuresdr::blocks::MessagePipe;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::Selector;
use futuresdr::blocks::SelectorDropPolicy as DropPolicy;
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

use wlan::fft_tag_propagation as wlan_fft_tag_propagation;
use wlan::parse_channel as wlan_parse_channel;
use wlan::Decoder as WlanDecoder;
use wlan::Delay as WlanDelay;
use wlan::Encoder as WlanEncoder;
//use wlan::FftShift;
use wlan::FrameEqualizer as WlanFrameEqualizer;
//use wlan::Keep1InN;
use wlan::Mac as WlanMac;
use wlan::Mapper as WlanMapper;
use wlan::Mcs as WlanMcs;
use wlan::MovingAverage as WlanMovingAverage;
use wlan::Prefix as WlanPrefix;
use wlan::SyncLong as WlanSyncLong;
use wlan::SyncShort as WlanSyncShort;
use wlan::MAX_SYM;

use zigbee::parse_channel as zigbee_parse_channel;
use zigbee::modulator as zigbee_modulator;
use zigbee::IqDelay as ZigbeeIqDelay;
use zigbee::Mac as ZigbeeMac;
use zigbee::ClockRecoveryMm as ZigbeeClockRecoveryMm;
use zigbee::Decoder as ZigbeeDecoder;


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
    /// TX Antenna
    #[clap(long)]
    tx_antenna: Option<String>,
    /// TX Soapy Filter
    #[clap(long)]
    tx_filter: Option<String>,
    /// Zigbee RX Gain
    #[clap(long, default_value_t = 50.0)]
    zigbee_rx_gain: f64,
    /// Zigbee TX Gain
    #[clap(long, default_value_t = 50.0)]
    zigbee_tx_gain: f64,
    /// Zigbee RX Channel
    #[clap(id = "zigbee-rx-channel", long, value_parser = zigbee_parse_channel, default_value = "26")]
    zigbee_rx_freq: f64,
    /// Zigbee TX Channel
    #[clap(id = "zigbee-tx-channel", long, value_parser = zigbee_parse_channel, default_value = "26")]
    zigbee_tx_freq: f64,
    /// Zigbee Sample Rate
    #[clap(long, default_value_t = 4e6)]
    zigbee_sample_rate: f64,
    /// WLAN RX Gain
    #[clap(long, default_value_t = 40.0)]
    wlan_rx_gain: f64,
    /// WLAN TX Gain
    #[clap(long, default_value_t = 40.0)]
    wlan_tx_gain: f64,
    /// WLAN Sample Rate
    #[clap(long, default_value_t = 20e6)]
    wlan_sample_rate: f64,
    /// WLAN RX Channel Number
    #[clap(long, value_parser = wlan_parse_channel, default_value = "34")]
    wlan_rx_channel: f64,
    /// WLAN TX Channel Number
    #[clap(long, value_parser = wlan_parse_channel, default_value = "34")]
    wlan_tx_channel: f64,
    /// Soapy RX Channel
    #[clap(long, default_value_t = 0)]
    soapy_rx_channel: usize,
    /// Soapy TX Channel
    #[clap(long, default_value_t = 0)]
    soapy_tx_channel: usize,
    /// TX MCS
    #[clap(long, value_parser = WlanMcs::parse, default_value = "qpsk12")]
    wlan_mcs: WlanMcs,
    /// local UDP port to receive messages to send
    #[clap(long, value_parser)]
    local_port: Option<u32>,
    /// local IP to bind to
    #[clap(long, value_parser, default_value = "0.0.0.0")]
    local_ip: String,
    /// remote UDP server to forward received messages to
    #[clap(long, value_parser)]
    remote_udp: Option<String>,
    /// send periodic messages for testing
    #[clap(long, value_parser)]
    tx_interval: Option<f32>,
    /// Stream Spectrum data at ws://0.0.0.0:9001
    #[clap(long, value_parser)]
    spectrum: bool,
    // Drop policy to apply on the selector.
    #[clap(short, long, default_value = "none")]
    drop_policy: DropPolicy,
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

    
    let rx_freq = [args.wlan_rx_channel, args.zigbee_rx_freq];
    let tx_freq = [args.wlan_tx_channel, args.zigbee_tx_freq];
    let rx_gain = [args.wlan_rx_gain, args.zigbee_rx_gain];
    let tx_gain = [args.wlan_tx_gain, args.zigbee_tx_gain];  
    let sample_rate = [args.wlan_sample_rate, args.zigbee_sample_rate];

    let mut fg = Flowgraph::new();
    
    let mut sink = SoapySinkBuilder::new()
        .freq(tx_freq[0])
        .sample_rate(sample_rate[0])
        .gain(tx_gain[0]);

    let mut src = SoapySourceBuilder::new()
        .freq(rx_freq[0])
        .sample_rate(sample_rate[0])
        .gain(rx_gain[0]);
        
    if let Some(f) = args.tx_filter {
        sink = sink.filter(f);
    }
    if let Some(f) = args.rx_filter {
        src = src.filter(f);
    }

    let sink = sink.build();
    let src = src.build();

    //message handler to change frequency and sample rate during runtime
    let sink_freq_input_port_id = sink
        .message_input_name_to_id("freq") 
        .expect("No freq port found!");
    let sink_sample_rate_input_port_id = sink
        .message_input_name_to_id("sample_rate")
        .expect("No sample_rate port found!");
    let sink_gain_input_port_id = sink
        .message_input_name_to_id("gain")
        .expect("No gain port found!");
    let sink = fg.add_block(sink);

    let src_freq_input_port_id = src
        .message_input_name_to_id("freq") 
        .expect("No freq port found!");
    let src_sample_rate_input_port_id = src
        .message_input_name_to_id("sample_rate")
        .expect("No sample_rate port found!");
    let src_gain_input_port_id = src
        .message_input_name_to_id("gain")
        .expect("No gain port found!");
    let src = fg.add_block(src);
    

    //Soapy Sink + Selector
    let sink_selector = Selector::<Complex32, 2, 1>::new(args.drop_policy);
    let input_index_port_id = sink_selector
        .message_input_name_to_id("input_index")
        .expect("No input_index port found!");
    let sink_selector = fg.add_block(sink_selector);
    fg.connect_stream(sink_selector, "out0", sink, "in")?;

    //source selector
    let src_selector = Selector::<Complex32, 1, 2>::new(args.drop_policy);
    let output_index_port_id = src_selector
        .message_input_name_to_id("output_index")
        .expect("No output_index port found!");
    let src_selector = fg.add_block(src_selector);
    fg.connect_stream(src, "out", src_selector, "in0")?;

    // ============================================
    // WLAN TRANSMITTER
    // ============================================
    let wlan_mac = fg.add_block(WlanMac::new([0x42; 6], [0x23; 6], [0xff; 6]));
    let wlan_encoder = fg.add_block(WlanEncoder::new(args.wlan_mcs));
    fg.connect_message(wlan_mac, "tx", wlan_encoder, "tx")?;
    let wlan_mapper = fg.add_block(WlanMapper::new());
    fg.connect_stream(wlan_encoder, "out", wlan_mapper, "in")?;
    let mut wlan_fft = Fft::with_options(
        64,
        FftDirection::Inverse,
        true,
        Some((1.0f32 / 52.0).sqrt()),
    );
    wlan_fft.set_tag_propagation(Box::new(wlan_fft_tag_propagation));
    let wlan_fft = fg.add_block(wlan_fft);
    fg.connect_stream(wlan_mapper, "out", wlan_fft, "in")?;
    let wlan_prefix = fg.add_block(WlanPrefix::new(PAD_FRONT, PAD_TAIL));
    fg.connect_stream_with_type(
        wlan_fft,
        "out",
        wlan_prefix,
        "in",
        Circular::with_size(prefix_in_size),
    )?;
    
    fg.connect_stream_with_type(
        wlan_prefix,
        "out",
        sink_selector,
        "in0",
        Circular::with_size(prefix_out_size),
    )?;

    // ============================================
    // WLAN RECEIVER
    // ============================================
    
    let wlan_delay = fg.add_block(WlanDelay::<Complex32>::new(16));
    fg.connect_stream(src_selector, "out0", wlan_delay, "in")?;

    let wlan_complex_to_mag_2 = fg.add_block(Apply::new(|i: &Complex32| i.norm_sqr()));
    let wlan_float_avg = fg.add_block(WlanMovingAverage::<f32>::new(64));
    fg.connect_stream(src_selector, "out0", wlan_complex_to_mag_2, "in")?;
    fg.connect_stream(wlan_complex_to_mag_2, "out", wlan_float_avg, "in")?;

    let wlan_mult_conj = fg.add_block(Combine::new(|a: &Complex32, b: &Complex32| a * b.conj()));
    let wlan_complex_avg = fg.add_block(WlanMovingAverage::<Complex32>::new(48));
    fg.connect_stream(src_selector, "out0", wlan_mult_conj, "in0")?;
    fg.connect_stream(wlan_delay, "out", wlan_mult_conj, "in1")?;
    fg.connect_stream(wlan_mult_conj, "out", wlan_complex_avg, "in")?;

    let wlan_divide_mag = fg.add_block(Combine::new(|a: &Complex32, b: &f32| a.norm() / b));
    fg.connect_stream(wlan_complex_avg, "out", wlan_divide_mag, "in0")?;
    fg.connect_stream(wlan_float_avg, "out", wlan_divide_mag, "in1")?;

    let wlan_sync_short = fg.add_block(WlanSyncShort::new());
    fg.connect_stream(wlan_delay, "out", wlan_sync_short, "in_sig")?;
    fg.connect_stream(wlan_complex_avg, "out", wlan_sync_short, "in_abs")?;
    fg.connect_stream(wlan_divide_mag, "out", wlan_sync_short, "in_cor")?;

    let wlan_sync_long = fg.add_block(WlanSyncLong::new());
    fg.connect_stream(wlan_sync_short, "out", wlan_sync_long, "in")?;

    let mut wlan_fft = Fft::new(64);
    wlan_fft.set_tag_propagation(Box::new(wlan_fft_tag_propagation));
    let wlan_fft = fg.add_block(wlan_fft);
    fg.connect_stream(wlan_sync_long, "out", wlan_fft, "in")?;

    let wlan_frame_equalizer = fg.add_block(WlanFrameEqualizer::new());
    fg.connect_stream(wlan_fft, "out", wlan_frame_equalizer, "in")?;

    let wlan_decoder = fg.add_block(WlanDecoder::new());
    fg.connect_stream(wlan_frame_equalizer, "out", wlan_decoder, "in")?;

    let (wlan_rxed_sender, mut wlan_rxed_frames) = mpsc::channel::<Pmt>(100);
    let wlan_message_pipe = fg.add_block(MessagePipe::new(wlan_rxed_sender));
    fg.connect_message(wlan_decoder, "rx_frames", wlan_message_pipe, "in")?;
    let wlan_blob_to_udp = fg.add_block(futuresdr::blocks::BlobToUdp::new("127.0.0.1:55555"));
    fg.connect_message(wlan_decoder, "rx_frames", wlan_blob_to_udp, "in")?;
    let wlan_blob_to_udp = fg.add_block(futuresdr::blocks::BlobToUdp::new("127.0.0.1:55556"));
    fg.connect_message(wlan_decoder, "rftap", wlan_blob_to_udp, "in")?;


    // ========================================
    // ZIGBEE RECEIVER
    // ========================================
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
    let mm = fg.add_block(ZigbeeClockRecoveryMm::new(
        omega,
        gain_omega,
        mu,
        gain_mu,
        omega_relative_limit,
    ));

    let zigbee_decoder = fg.add_block(ZigbeeDecoder::new(6));
    let zigbee_mac = fg.add_block(ZigbeeMac::new());
    let null_sink = fg.add_block(NullSink::<u8>::new());
    let zigbee_blob_to_udp = fg.add_block(futuresdr::blocks::BlobToUdp::new("127.0.0.1:55557"));
    //let (zigbee_rxed_sender, mut zigbee_rxed_frames) = mpsc::channel::<Pmt>(100);
    //let zigbee_message_pipe = fg.add_block(MessagePipe::new(zigbee_rxed_sender));

    fg.connect_stream(src_selector, "out1", avg, "in")?;
    fg.connect_stream(avg, "out", mm, "in")?;
    fg.connect_stream(mm, "out", zigbee_decoder, "in")?;
    //fg.connect_stream(zigbee_mac, "out", zigbee_message_pipe, "in")?;
    fg.connect_stream(zigbee_mac, "out", null_sink, "in")?;
    fg.connect_message(zigbee_decoder, "out", zigbee_mac, "rx")?;
    fg.connect_message(zigbee_mac, "out", zigbee_blob_to_udp, "in")?;


    // ========================================
    // ZIGBEE TRANSMITTER
    // ========================================

    let zigbee_mac = fg.add_block(ZigbeeMac::new());
    let zigbee_modulator = fg.add_block(zigbee_modulator());
    let zigbee_iq_delay = fg.add_block(ZigbeeIqDelay::new());

    fg.connect_stream(zigbee_mac, "out", zigbee_modulator, "in")?;
    fg.connect_stream(zigbee_modulator, "out", zigbee_iq_delay, "in")?;
    fg.connect_stream(zigbee_iq_delay, "out", sink_selector, "in1")?;

    // ========================================
    // Spectrum
    // ========================================
    // if args.spectrum {
    //     let snk = fg.add_block(
    //         WebsocketSinkBuilder::<f32>::new(9001)
    //             .mode(WebsocketSinkMode::FixedDropping(2048))
    //             .build(),
    //     );
    //     let fft = fg.add_block(Fft::new(2048));
    //     let shift = fg.add_block(FftShift::new());
    //     let keep = fg.add_block(Keep1InN::new(0.5, 10));
    //     let cpy = fg.add_block(futuresdr::blocks::Copy::<Complex32>::new());

    //     fg.connect_stream(src, "out", cpy, "in")?;
    //     fg.connect_stream(cpy, "out", fft, "in")?;
    //     fg.connect_stream(fft, "out", shift, "in")?;
    //     fg.connect_stream(shift, "out", keep, "in")?;
    //     fg.connect_stream(keep, "out", snk, "in")?;
    // }

    let rt = Runtime::new();
    let (_fg, mut handle) = block_on(rt.start(fg));
    let (mode_sender, mode_receiver) = channel();
    let (udp_client_mode_sender, udp_client_mode_receiver) = channel();
    let (udp_server_mode_sender, udp_server_mode_receiver) = channel();
    let mut input_handle = handle.clone();
    let mut mode = 0;  

    // if tx_interval is set, send messages periodically
    if let Some(tx_interval) = args.tx_interval {
        let mut seq = 0u64;
        let mut myhandle = handle.clone();
        rt.spawn_background(async move {
            loop {
                Timer::after(Duration::from_secs_f32(tx_interval)).await;
                if let Some(new_mode) = mode_receiver.try_recv().ok(){
                    mode = new_mode;
                }
                if mode == 0 {
                    myhandle
                    .call(
                        wlan_mac,
                        0,
                        Pmt::Blob(format!("FutureSDR {}", seq).as_bytes().to_vec()),
                    )
                    .await
                    .unwrap();
                seq += 1;
                }
                if mode == 1 {
                    myhandle
                    .call(
                        zigbee_mac,
                        1,
                        Pmt::Blob(format!("FutureSDR {}", seq).as_bytes().to_vec()),
                    )
                    .await
                    .unwrap();
                seq += 1;
                }
                
            }
        });
    }

    // we are the udp server
    if let Some(port) = args.local_port {
        info!("Acting as UDP server.");
        let (tx_endpoint, rx_endpoint) = oneshot::channel::<SocketAddr>();
        let socket = block_on(UdpSocket::bind(format!("{}:{}", args.local_ip, port))).unwrap();
        let socket2 = socket.clone();

        rt.spawn_background(async move {
            let mut buf = vec![0u8; 1024];

            let (n, e) = socket.recv_from(&mut buf).await.unwrap();
            if let Some(new_mode) = udp_server_mode_receiver.try_recv().ok(){
                mode = new_mode;
            }
            if mode == 0 {
                handle
                    .call(wlan_mac, 0, Pmt::Blob(buf[0..n].to_vec()))
                    .await
                    .unwrap();
            }
            if mode == 1 {
                handle
                    .call(zigbee_mac, 1, Pmt::Blob(buf[0..n].to_vec()))
                    .await
                    .unwrap();
            }
            tx_endpoint.send(e).unwrap();

            loop {
                let (n, _) = socket.recv_from(&mut buf).await.unwrap();
                if let Some(new_mode) = udp_server_mode_receiver.try_recv().ok(){
                    mode = new_mode;
                }
                if mode == 0 {
                    handle
                        .call(wlan_mac, 0, Pmt::Blob(buf[0..n].to_vec()))
                        .await
                        .unwrap();
                }
                if mode == 1 {
                    handle
                        .call(zigbee_mac, 1, Pmt::Blob(buf[0..n].to_vec()))
                        .await
                        .unwrap();
                }
            }
        });

        rt.spawn_background(async move {
            let endpoint = rx_endpoint.await.unwrap();
            info!("endpoint connected to local udp server {:?}", &endpoint);

            loop {
                if let Some(p) = wlan_rxed_frames.next().await {
                    if let Pmt::Blob(v) = p {
                        println!("received frame, size {}", v.len() - 24);
                        socket2.send_to(&v[24..], endpoint).await.unwrap();
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
        let socket = block_on(UdpSocket::bind(format!("{}:{}", args.local_ip, 0))).unwrap();
        block_on(socket.connect(remote)).unwrap();
        let socket2 = socket.clone();

        // rt.spawn_background(async move {
        //     let mut buf = vec![0u8; 1024];
        //     loop {
        //         match socket.recv_from(&mut buf).await {
        //             Ok((n, s)) => {
        //                 if let Some(new_mode) = udp_client_mode_receiver.try_recv().ok(){
        //                     mode = new_mode;
        //                 }
        //                 if mode == 0 {
        //                     println!("sending frame size {} from {:?} in WLAN mode", n, s);
        //                     handle
        //                         .call(wlan_mac, 0, Pmt::Blob(buf[0..n].to_vec()))
        //                         .await
        //                         .unwrap()
        //                 }
        //                 if mode == 1 {
        //                     println!("sending frame size {} from {:?} in Zigbee mode", n, s);
        //                     handle
        //                         .call(zigbee_mac, 1, Pmt::Blob(buf[0..n].to_vec()))
        //                         .await
        //                         .unwrap()
        //                 }
        //             }
        //             Err(e) => println!("ERROR: {:?}", e),
        //         }
        //     }
        // });

        rt.spawn_background(async move {
            loop {
                if let Some(p) = wlan_rxed_frames.next().await {
                    if let Pmt::Blob(v) = p {
                        println!("received frame size {}", v.len() - 24);
                        socket2.send(&v[24..]).await.unwrap();
                    } else {
                        warn!("pmt to tx was not a blob");
                    }
                // } else if let Some(p) = zigbee_rxed_frames.next().await {
                //     if let Pmt::Blob(v) = p {
                //         println!("received frame size {}", v.len() - 24);
                //         socket2.send(&v[0..]).await.unwrap();
                //     } else {
                //         warn!("pmt to tx was not a blob");
                //     }
                } else {
                    warn!("cannot read from MessagePipe receiver");
                }
            }
        });
    } else {
        info!("No UDP forwarding configured");
        rt.spawn_background(async move {
            loop {
                if let Some(p) = wlan_rxed_frames.next().await {
                    if let Pmt::Blob(v) = p {
                        println!("received frame size {}", v.len() - 24);
                    } else {
                        warn!("pmt to tx was not a blob");
                    }
                } else {
                    warn!("cannot read from MessagePipe receiver");
                }
            }
        });
    }

    // Keep asking user for the selection
    loop {
        println!("Enter a new output index");
        // Get input from stdin and remove all whitespace (most importantly '\n' at the end)
        let mut input = String::new(); // Input buffer
        std::io::stdin()
            .read_line(&mut input)
            .expect("error: unable to read user input");
        input.retain(|c| !c.is_whitespace());

        // If the user entered a valid number, set the new frequency, gain and sample rate by sending a message to the `FlowgraphHandle`
        if let Ok(new_index) = input.parse::<u32>() {
            println!("Setting source index to {}", input);
            mode_sender.send(new_index)?;
            udp_client_mode_sender.send(new_index)?;
            udp_server_mode_sender.send(new_index)?;

            async_io::block_on(
                input_handle
                .call(
                    src, 
                    src_freq_input_port_id, 
                    Pmt::F64(rx_freq[new_index as usize])
                )
            )?;
            async_io::block_on(
                input_handle
                .call(
                    src, 
                    src_sample_rate_input_port_id, 
                    Pmt::F64(sample_rate[new_index as usize])
                )
            )?;
            async_io::block_on(
                input_handle
                .call(
                    src, 
                    src_gain_input_port_id, 
                    Pmt::F64(rx_gain[new_index as usize])
                )
            )?;
            async_io::block_on(
                input_handle
                .call(
                    src_selector, 
                    output_index_port_id, 
                    Pmt::U32(new_index)
                )
            )?;
            async_io::block_on(
                input_handle
                    .call(
                        sink, 
                        sink_freq_input_port_id, 
                        Pmt::F64(tx_freq[new_index as usize])
                    )
            )?;
            async_io::block_on(
                input_handle
                    .call(
                        sink, 
                        sink_sample_rate_input_port_id, 
                        Pmt::F64(sample_rate[new_index as usize])
                    )
            )?;
            async_io::block_on(
                input_handle
                    .call(
                        sink, 
                        sink_gain_input_port_id, 
                        Pmt::F64(tx_gain[new_index as usize])
                    )
            )?;
            async_io::block_on(
                input_handle
                    .call(
                        sink_selector, 
                        input_index_port_id, 
                        Pmt::U32(new_index)
                    )
            )?;
        } else {
            println!("Input not parsable: {}", input);
        }
    }

}
