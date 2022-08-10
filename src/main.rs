use anyhow::Result;
use std::{io::Write, time::Duration};
use std::sync::Arc;
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

/// must_read_stdin blocks until input is received from stdin
pub fn must_read_stdin() -> Result<String> {
    let mut line = String::new();

    std::io::stdin().read_line(&mut line)?;
    line = line.trim().to_owned();
    println!();

    Ok(line)
}
pub fn encode(b: &str) -> String {
    base64::encode(b)
}

pub fn decode(s: &str) -> Result<String> {
    let b = base64::decode(s)?;
    let s = String::from_utf8(b)?;
    Ok(s)
}

#[tokio::main]
async fn main() -> Result<()> {
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
    let width: u32 = *matches.get_one("width").expect("Parameter `width` is required. ");
    let height: u32 = *matches.get_one("height").expect("Parameter `height` is required. ");

    if debug {
        env_logger::Builder::new()
            .format(|buf, record| {
                writeln!(
                    buf,
                    "{}:{} [{}] {} - {}",
                    record.file().unwrap_or("unknown"),
                    record.line().unwrap_or(0),
                    record.level(),
                    chrono::Local::now().format("%H:%M:%S.%6f"),
                    record.args()
                )
            })
            .filter(None, log::LevelFilter::Trace)
            .init();
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

    tokio::spawn(async move {
        let mut rtcp_buf = vec![0u8; 1500];
        while let Ok((_, _)) = rtp_sender.read(&mut rtcp_buf).await {}
        Result::<()>::Ok(())
    });

    tokio::spawn(async move {
        // TODO: Interact with mdance io
        let mut ticker = tokio::time::interval(Duration::from_millis(16));
        let mut client = mdanceio::offscreen_proxy::OffscreenProxy::init(width, height).await;

        loop {
            let frame = client.redraw();
            let _ = ticker.tick().await;
        }
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
    let line = must_read_stdin()?;
    let desc_data = decode(line.as_str())?;
    let offer = serde_json::from_str::<RTCSessionDescription>(&desc_data)?;

    peer_connection.set_remote_description(offer).await?;

    let answer = peer_connection.create_answer(None).await?;

    let mut gather_complete = peer_connection.gathering_complete_promise().await;

    peer_connection.set_local_description(answer).await?;

    let _ = gather_complete.recv().await.unwrap();

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
