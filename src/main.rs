use anyhow::Result;
use dcv_color_primitives as dcp;
use std::sync::Arc;
use std::time::Instant;
use std::{io::Write, time::Duration};
use tokio::sync::Notify;
use webrtc::{
    api::{
        interceptor_registry::register_default_interceptors,
        media_engine::{MediaEngine, MIME_TYPE_VP8},
        APIBuilder,
    },
    ice_transport::{ice_connection_state::RTCIceConnectionState, ice_server::RTCIceServer},
    interceptor::registry::Registry,
    peer_connection::{
        configuration::RTCConfiguration, peer_connection_state::RTCPeerConnectionState,
        sdp::session_description::RTCSessionDescription,
    },
    rtp_transceiver::rtp_codec::RTCRtpCodecCapability,
    track::track_local::{track_local_static_sample::TrackLocalStaticSample, TrackLocal},
};

pub fn encode(b: &str) -> String {
    base64::encode(b)
}

pub fn decode(s: &str) -> Result<String> {
    let b = base64::decode(s)?;
    let s = String::from_utf8(b)?;
    Ok(s)
}

pub fn load_sample_data(client: &mut mdanceio::offscreen_proxy::OffscreenProxy) -> Result<()> {
    let model_data = std::fs::read("private_data/Alicia/MMD/Alicia_solid.pmx")?;
    client.load_model(&model_data);
    drop(model_data);
    let texture_dir = std::fs::read_dir("private_data/Alicia/FBX/").unwrap();
    for texture_file in texture_dir {
        let texture_file = texture_file.unwrap();
        let texture_data = std::fs::read(texture_file.path())?;
        client.load_texture(
            texture_file.file_name().to_str().unwrap(),
            &texture_data[..],
            true,
        );
    }
    let motion_data = std::fs::read("private_data/Alicia/MMD Motion/2 for test 1.vmd")?;
    client.load_model_motion(&motion_data);
    client.disable_physics_simulation();
    drop(motion_data);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    dcp::initialize();
    let mut app = clap::App::new("mdrs")
        .version("0.1.0")
        .author("NAiveD <nice-die@live.com>")
        .setting(clap::AppSettings::DeriveDisplayOrder)
        .arg(
            clap::Arg::new("debug")
                .long("debug")
                .short('d')
                .help("Prints debug message"),
        )
        .arg(
            clap::Arg::new("width")
                .long("width")
                .short('w')
                .default_value("800")
                .value_parser(clap::value_parser!(u32).range(1..))
                .help("Width of render target texture"),
        )
        .arg(
            clap::Arg::new("height")
                .long("height")
                .short('h')
                .default_value("600")
                .value_parser(clap::value_parser!(u32).range(1..))
                .help("Height of render target texture"),
        );

    let matches = app.clone().get_matches();
    let debug = matches.is_present("debug");
    let width: u32 = *matches
        .get_one("width")
        .expect("Parameter `width` is required. ");
    let height: u32 = *matches
        .get_one("height")
        .expect("Parameter `height` is required. ");

    if debug {
        let logfile = log4rs::append::file::FileAppender::builder()
            .encoder(Box::new(log4rs::encode::pattern::PatternEncoder::new(
                "{d(%Y-%m-%d %H:%M:%S.%6f)} [{level}] - {m} [{file}:{line}]{n}",
            )))
            .build("target/log/output.log")?;

        let config = log4rs::config::Config::builder()
            .appender(log4rs::config::Appender::builder().build("logfile", Box::new(logfile)))
            .build(
                log4rs::config::Root::builder()
                    .appender("logfile")
                    .build(log::LevelFilter::Info),
            )?;

        log4rs::init_config(config)?;
    }

    let mut media_engine = MediaEngine::default();
    media_engine.register_default_codecs()?;

    let mut registry = Registry::new();
    registry = register_default_interceptors(registry, &mut media_engine)?;

    let api = APIBuilder::new()
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .build();

    let config = RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_owned()],
            ..Default::default()
        }],
        ..Default::default()
    };

    let peer_connection = Arc::new(api.new_peer_connection(config).await?);

    let notify_tx = Arc::new(Notify::new());
    let notify_video = notify_tx.clone();

    let (done_tx, mut done_rx) = tokio::sync::mpsc::channel(1);
    let video_done_tx = done_tx.clone();

    let video_track = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "mdrs".to_owned(),
    ));

    let rtp_sender = peer_connection
        .add_track(Arc::clone(&video_track) as Arc<dyn TrackLocal + Send + Sync>)
        .await?;

    let mut vpx = vpx_encode::Encoder::new(vpx_encode::Config {
        width,
        height,
        timebase: [1, 1000],
        bitrate: 250,
        codec: vpx_encode::VideoCodecId::VP8,
    })?;

    tokio::spawn(async move {
        let mut rtcp_buf = vec![0u8; 1500];
        while let Ok((_, _)) = rtp_sender.read(&mut rtcp_buf).await {}
        Result::<()>::Ok(())
    });

    tokio::spawn(async move {
        // TODO: Interact with mdance io
        let mut client = mdanceio::offscreen_proxy::OffscreenProxy::init(width, height).await;
        load_sample_data(&mut client)?;

        let _ = notify_video.notified().await;
        println!("Video Notified");

        let start = Instant::now();
        let mut ticker = tokio::time::interval(Duration::from_millis(16));
        client.play();

        loop {
            let (width, height) = client.viewport_size();
            let src_format = dcp::ImageFormat {
                pixel_format: dcp::PixelFormat::Bgra,
                color_space: dcp::ColorSpace::Rgb,
                num_planes: 1,
            };
            const DST_NUM_PLANES: usize = 3;
            let dst_format = dcp::ImageFormat {
                pixel_format: dcp::PixelFormat::I420,
                color_space: dcp::ColorSpace::Bt601,
                num_planes: DST_NUM_PLANES as u32,
            };
            let mut buffers_size = [0usize; DST_NUM_PLANES];
            if let Err(e) =
                dcp::get_buffers_size(width, height, &dst_format, None, &mut buffers_size)
            {
                log::error!("Getting Buffer Size Error: {:?}", e);
                break;
            }
            println!("Buffers Size: {:?}", buffers_size);
            let mut dst_buffers = buffers_size.map(|size| vec![0u8; size]).to_vec();
            let mut dst_buffers: Vec<_> = dst_buffers.iter_mut().map(|v| &mut v[..]).collect();

            println!("Start Drawing frame");
            let offset = Instant::now() - start;
            let frame = client.redraw();
            println!("Start Converting to I420");
            if let Err(e) = dcp::convert_image(
                width,
                height,
                &src_format,
                None,
                &[&frame[..]],
                &dst_format,
                None,
                &mut dst_buffers,
            ) {
                log::error!("Error On Convert Frame: {:?}", e);
                break;
            }

            let mut dst_buffer = vec![];
            for plane in &dst_buffers {
                dst_buffer.extend(plane.iter());
            }
            for packet in vpx.encode(offset.as_millis() as i64, &dst_buffer[..])? {
                println!("Start Send Frame");
                video_track
                    .write_sample(&webrtc::media::Sample {
                        data: bytes::Bytes::copy_from_slice(packet.data),
                        duration: Duration::from_millis(16),
                        ..Default::default()
                    })
                    .await?;
            }

            println!("Finish One frame");
            let _ = ticker.tick().await;
        }

        let _ = video_done_tx.try_send(());
        Result::<()>::Ok(())
    });

    peer_connection
        .on_ice_connection_state_change(Box::new(move |connection_state: RTCIceConnectionState| {
            log::info!("Connection State Changed {}", connection_state);
            if connection_state == RTCIceConnectionState::Connected {
                notify_tx.notify_waiters();
            }
            Box::pin(async {})
        }))
        .await;

    peer_connection
        .on_peer_connection_state_change(Box::new(
            move |connection_state: RTCPeerConnectionState| {
                log::info!("Peer Connection State Changed {}", connection_state);
                if connection_state == RTCPeerConnectionState::Failed {
                    let _ = done_tx.try_send(());
                }
                Box::pin(async {})
            },
        ))
        .await;

    // Wait for the offer to be pasted
    let line = include_str!("../private_data/session_desc.txt");
    let desc_data = decode(line)?;
    let offer = serde_json::from_str::<RTCSessionDescription>(&desc_data)?;

    peer_connection.set_remote_description(offer).await?;

    let answer = peer_connection.create_answer(None).await?;

    let mut gather_complete = peer_connection.gathering_complete_promise().await;

    peer_connection.set_local_description(answer).await?;

    let _ = gather_complete.recv().await;

    if let Some(local_desc) = peer_connection.local_description().await {
        let json_str = serde_json::to_string(&local_desc)?;
        let b64 = base64::encode(&json_str);
        println!("{}", b64);
    } else {
        println!("generate local_description failed!");
    }

    println!("Press ctrl-c to stop");
    tokio::select! {
        _ = done_rx.recv() => {
            println!("received done signal!");
        }
        _ = tokio::signal::ctrl_c() => {
            println!("");
        }
    };

    peer_connection.close().await?;
    Ok(())
}
