use bytes::Bytes;
use echo_audio::{AudioController, ChannelStreamReader, StreamingAudioEngine};
use std::io::Read;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

mod common;
use common::*;

#[tokio::test]
async fn test_offline_headless_decode_synthetic_wav() {
    let controller = Arc::new(AudioController::new());
    let engine = StreamingAudioEngine::new(controller);

    let wav_data = create_synthetic_wav(44100, 2, 500, 440.0);
    let cursor = std::io::Cursor::new(wav_data);

    let stop_flag = Arc::new(AtomicBool::new(false));
    let decoded = engine
        .test_decode_reader(cursor, 10, stop_flag)
        .await
        .expect("Headless decode of synthetic WAV must succeed");

    assert!(decoded >= 1, "Should have decoded at least 1 audio packet");
}

#[tokio::test]
async fn test_offline_http_loopback_streaming() {
    let wav_data = create_synthetic_wav(48000, 2, 1000, 1000.0);
    let server = MockAudioServer::spawn(wav_data, 1024, "audio/vnd.wave").await;

    let controller = Arc::new(AudioController::new());
    let engine = StreamingAudioEngine::new(controller);

    let decoded = engine
        .test_decode_stream(&server.url(), 5)
        .await
        .expect("StreamingAudioEngine must decode stream over loopback HTTP offline");

    assert!(decoded >= 5, "Should decode at least 5 packets");
}

#[test]
fn test_offline_channel_stream_reader_buffering() {
    let (tx, rx) = tokio::sync::mpsc::channel::<Bytes>(16);

    let sender_thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let chunk1 = Bytes::from_static(b"Hello ");
            let chunk2 = Bytes::from_static(b"World!");
            tx.send(chunk1).await.unwrap();
            tx.send(chunk2).await.unwrap();
            drop(tx);
        });
    });

    let mut reader = ChannelStreamReader::new(rx);
    let mut out = String::new();
    reader.read_to_string(&mut out).unwrap();
    sender_thread.join().unwrap();

    assert_eq!(out, "Hello World!");
}

#[test]
fn test_synthetic_adts_aac_generation() {
    let adts = create_synthetic_adts_aac(5, 4, 2);
    // Each frame header starts with 0xFFF sync word
    assert!(!adts.is_empty());
    assert_eq!(adts[0], 0xFF);
    assert_eq!(adts[1] & 0xF0, 0xF0);
}
