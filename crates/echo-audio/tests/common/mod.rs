use std::f32::consts::PI;
use std::net::SocketAddr;

/// Generates a valid RIFF WAVE in-memory byte buffer containing 16-bit PCM samples.
pub fn create_synthetic_wav(
    sample_rate: u32,
    channels: u16,
    duration_ms: u32,
    freq_hz: f32,
) -> Vec<u8> {
    let num_samples = (sample_rate as u64 * duration_ms as u64 / 1000) as usize;
    let bytes_per_sample = 2usize;
    let block_align = channels as usize * bytes_per_sample;
    let data_bytes = num_samples * block_align;
    let total_file_size = 36 + data_bytes;

    let mut buf = Vec::with_capacity(total_file_size + 8);
    // RIFF chunk descriptor
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(total_file_size as u32).to_le_bytes());
    buf.extend_from_slice(b"WAVE");

    // "fmt " sub-chunk
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // subchunk1 size (16 for PCM)
    buf.extend_from_slice(&1u16.to_le_bytes()); // AudioFormat 1 = PCM
    buf.extend_from_slice(&channels.to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    let byte_rate = sample_rate * block_align as u32;
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&(block_align as u16).to_le_bytes());
    buf.extend_from_slice(&16u16.to_le_bytes()); // bits per sample

    // "data" sub-chunk
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&(data_bytes as u32).to_le_bytes());

    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let sample = (f32::sin(2.0 * PI * freq_hz * t) * (i16::MAX as f32 * 0.8)) as i16;
        for _ in 0..channels {
            buf.extend_from_slice(&sample.to_le_bytes());
        }
    }

    buf
}

/// Generates synthetic ADTS frames with AAC-LC sync word and frame headers.
pub fn create_synthetic_adts_aac(
    num_frames: usize,
    sample_rate_idx: u8,
    channel_cfg: u8,
) -> Vec<u8> {
    let raw_frame_payload = [0x21, 0x10, 0x05, 0x20, 0x80]; // Minimal AAC-LC frame body
    let frame_len = 7 + raw_frame_payload.len();
    let mut out = Vec::with_capacity(num_frames * frame_len);

    for _ in 0..num_frames {
        let b0 = 0xFF;
        let b1 = 0xF1; // MPEG-4, layer 0, no CRC
        let profile = 1u8; // AAC-LC (2 - 1)
        let b2 = (profile << 6) | ((sample_rate_idx & 0x0F) << 2) | ((channel_cfg >> 2) & 0x01);
        let b3 = ((channel_cfg & 0x03) << 6) | (((frame_len >> 11) & 0x03) as u8);
        let b4 = ((frame_len >> 3) & 0xFF) as u8;
        let b5 = (((frame_len & 0x07) << 5) | 0x1F) as u8;
        let b6 = 0xFC; // buffer fullness + 1 raw frame

        out.extend_from_slice(&[b0, b1, b2, b3, b4, b5, b6]);
        out.extend_from_slice(&raw_frame_payload);
    }
    out
}

/// In-process HTTP mock streaming server on 127.0.0.1 for 100% offline integration tests.
pub struct MockAudioServer {
    addr: SocketAddr,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl MockAudioServer {
    pub async fn spawn(data: Vec<u8>, chunk_size: usize, mime: &str) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        let mime = mime.to_string();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        break;
                    }
                    res = listener.accept() => {
                        match res {
                            Ok((mut socket, _)) => {
                                let data = data.clone();
                                let mime = mime.clone();
                                tokio::spawn(async move {
                                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                                    let mut req = [0u8; 2048];
                                    let _ = socket.read(&mut req).await;
                                    let header = format!(
                                        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                        mime, data.len()
                                    );
                                    let _ = socket.write_all(header.as_bytes()).await;
                                    for chunk in data.chunks(chunk_size) {
                                        if socket.write_all(chunk).await.is_err() {
                                            break;
                                        }
                                        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                                    }
                                });
                            }
                            Err(_) => break,
                        }
                    }
                }
            }
        });

        Self {
            addr,
            shutdown_tx: Some(shutdown_tx),
        }
    }

    pub fn url(&self) -> String {
        format!("http://{}/audio", self.addr)
    }
}

impl Drop for MockAudioServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}
