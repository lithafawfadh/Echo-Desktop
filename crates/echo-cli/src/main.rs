//! Echo Desktop — Live CLI music player (Milestone 3)
//!
//! Usage: echo "bohemian rhapsody queen"
//!
//! Feature flags:
//!   live-audio  — gates actual AudioEngine calls (requires Milestone 2 complete)
//!
//! Without `live-audio` the binary runs in **dry-run** mode: it searches,
//! resolves the stream URL, prints it, and exits cleanly.

use echo_core::AudioCodec;
use echo_provider::YouTubeMusicClient;

// Items used only in the live-audio feature path — suppress dead-code lint when
// that feature is disabled (which it is by default while Milestone 2 is pending).
#[cfg(feature = "live-audio")]
use {
    echo_audio::AudioController,
    std::io::{self, BufRead, Write},
    std::sync::Arc,
    std::time::Duration,
    tracing::warn,
};

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

fn format_duration(total_seconds: u32) -> String {
    let m = total_seconds / 60;
    let s = total_seconds % 60;
    format!("{m}:{s:02}")
}

#[cfg(feature = "live-audio")]
fn format_ms(ms: u64) -> String {
    let total = (ms / 1000) as u32;
    format_duration(total)
}

fn codec_label(codec: AudioCodec) -> &'static str {
    match codec {
        AudioCodec::Opus => "Opus",
        AudioCodec::Aac => "AAC",
        AudioCodec::Flac => "FLAC",
        AudioCodec::Mp3 => "MP3",
        AudioCodec::Unknown => "Unknown",
    }
}

/// Renders a progress bar of `width` chars showing `fraction` (0.0 – 1.0).
#[cfg(feature = "live-audio")]
fn progress_bar(fraction: f64, width: usize) -> String {
    let filled = ((fraction.clamp(0.0, 1.0) * width as f64) as usize).min(width);
    let empty = width - filled;
    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}

// ──────────────────────────────────────────────────────────────────────────────
// Entry point
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_target(false)
        .init();

    std::process::exit(run().await);
}

async fn run() -> i32 {
    // ── 1. Parse CLI args ────────────────────────────────────────────────────
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("Usage: echo <search query | --test-stream>");
        eprintln!("  e.g. echo \"bohemian rhapsody queen\"");
        eprintln!("  e.g. echo --test-stream   (plays live audio demo via CPAL & Symphonia)");
        return 1;
    }

    // Direct audio pipeline live demo
    if args.len() == 1 && args[0] == "--test-stream" {
        println!("Echo Desktop — Live Audio Engine Test");
        println!("{}", "=".repeat(40));
        let demo_url = "https://www.soundhelix.com/examples/mp3/SoundHelix-Song-1.mp3";
        println!("▶  Streaming test audio (SoundHelix Song 1 — High-Fidelity Audio)");
        println!("   Source: {demo_url}");
        println!("   Decoding via Symphonia -> SPSC Ring Buffer -> CPAL WASAPI");
        println!("   {}", "─".repeat(40));

        #[cfg(feature = "live-audio")]
        {
            use echo_audio::{AudioController, StreamingAudioEngine};
            use std::io::{self, BufRead};

            let controller = Arc::new(AudioController::new());
            let engine = StreamingAudioEngine::new(controller.clone());
            let play_url = demo_url.to_string();

            let play_handle = tokio::spawn(async move {
                if let Err(e) = engine.play_stream(&play_url).await {
                    eprintln!("Audio playback error: {e}");
                }
            });

            // Telemetry progress loop
            let ctrl_progress = Arc::clone(&controller);
            let progress_handle = tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    if !ctrl_progress.is_playing() {
                        break;
                    }
                    let pos = ctrl_progress.position_ms();
                    let dur = 198_000u64; // ~3m 18s
                    let fraction = pos as f64 / dur as f64;
                    let bar = progress_bar(fraction, 25);
                    print!("\r▶  {} / {} {bar}  ", format_ms(pos), format_ms(dur));
                    let _ = io::stdout().flush();
                }
                println!();
            });

            println!("   Press Enter to stop playback.\n");
            {
                let stdin = io::stdin();
                let _ = stdin.lock().lines().next();
            }

            controller.set_playing(false);
            play_handle.abort();
            progress_handle.abort();
            tokio::time::sleep(Duration::from_millis(100)).await;

            println!("\n⏹  Playback stopped cleanly.");
            return 0;
        }

        #[cfg(not(feature = "live-audio"))]
        {
            println!("   Please compile with --features live-audio to enable output device playback.");
            return 0;
        }
    }

    let query = args.join(" ");

    // ── Banner ───────────────────────────────────────────────────────────────
    println!("Echo Desktop — Ad-Free Music Streaming");
    println!("{}", "=".repeat(40));

    // ── 2. Create provider ───────────────────────────────────────────────────
    let client = match YouTubeMusicClient::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("❌  Failed to initialise provider: {e}");
            return 1;
        }
    };

    // ── 3. Search ────────────────────────────────────────────────────────────
    println!("🔍 Searching for: {query}");
    let tracks = match client.search_tracks(&query).await {
        Ok(t) if !t.is_empty() => t,
        Ok(_) => {
            eprintln!("❌  No results found for \"{query}\"");
            return 1;
        }
        Err(e) => {
            eprintln!("❌  Search failed: {e}");
            return 1;
        }
    };

    // ── Display results (up to 10) ───────────────────────────────────────────
    for (i, track) in tracks.iter().take(10).enumerate() {
        println!(
            "  [{n}] {title} — {artist} ({dur})",
            n = i + 1,
            title = track.title,
            artist = track.artist,
            dur = format_duration(track.duration_seconds),
        );
    }
    println!();

    // ── 4. Auto-select first result ──────────────────────────────────────────
    let track = &tracks[0];

    // ── 5. Resolve stream ────────────────────────────────────────────────────
    println!("⏳ Resolving stream for: {} — {}", track.title, track.artist);
    let stream = match client.resolve_stream_url(&track.id).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("❌  Stream resolution failed: {e}");
            return 1;
        }
    };

    println!(
        "   Stream: {codec} {kbps}kbps | {sr}Hz",
        codec = codec_label(stream.codec),
        kbps = stream.bitrate_kbps,
        sr = stream.sample_rate_hz,
    );

    // ── Dry-run mode (live-audio feature not enabled) ────────────────────────
    #[cfg(not(feature = "live-audio"))]
    {
        println!();
        println!("▶  [dry-run] Resolved stream URL:");
        println!("   {}", stream.url);
        println!();
        println!(
            "   (Compile with --features live-audio to enable real playback once Milestone 2 is complete)"
        );
        return 0;
    }

    // ── Live audio path (requires Milestone 2 — AudioEngine) ─────────────────
    #[cfg(feature = "live-audio")]
    {
        use echo_audio::StreamingAudioEngine;

        println!();
        println!("▶  Playing: {} — {}", track.title, track.artist);
        println!("   {}", "─".repeat(34));

        let controller = Arc::new(AudioController::new());
        let engine = StreamingAudioEngine::new(controller.clone());
        let stream_url = stream.url.clone();

        let play_handle = tokio::spawn(async move {
            if let Err(e) = engine.play_stream(&stream_url).await {
                warn!("Audio engine error: {e}");
            }
        });

        // ── 8. Progress display (every 500 ms) ───────────────────────────────
        let ctrl_progress = Arc::clone(&controller);
        let duration_ms = (track.duration_seconds as u64) * 1000;
        let progress_handle = tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;
                if !ctrl_progress.is_playing() {
                    break;
                }
                let pos = ctrl_progress.position_ms();
                let dur = if ctrl_progress.duration_ms() > 0 {
                    ctrl_progress.duration_ms()
                } else {
                    duration_ms
                };
                let fraction = if dur > 0 { pos as f64 / dur as f64 } else { 0.0 };
                let bar = progress_bar(fraction, 20);
                print!("\r▶  {} / {} {bar}  ", format_ms(pos), format_ms(dur));
                let _ = io::stdout().flush();
            }
            println!();
        });

        // ── 9. Wait for Enter to stop ─────────────────────────────────────────
        println!("   Press Enter to stop.\n");
        {
            let stdin = io::stdin();
            let _ = stdin.lock().lines().next();
        }

        controller.set_playing(false);
        play_handle.abort();
        progress_handle.abort();
        tokio::time::sleep(Duration::from_millis(100)).await;

        println!("\n⏹  Playback stopped.");
        return 0;
    }

    #[allow(unreachable_code)]
    0
}
