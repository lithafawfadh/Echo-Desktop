use echo_lyrics::LrcLibClient;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    info!("Starting Echo Music Engine (100% Rust / D: Drive)");

    let lyrics_client = LrcLibClient::new();

    println!("==================================================");
    println!(" Echo Desktop Engine - Proof of Concept");
    println!("==================================================");

    // Demonstration of real-time synchronized lyrics fetching
    println!("Fetching synchronized lyrics for demonstration...");
    match lyrics_client.fetch_lyrics("Starboy", "The Weeknd", Some(230)).await {
        Ok(lyrics) => {
            println!("Lyrics fetched successfully!");
            if let Some(first_line) = lyrics.synced.first() {
                println!("First lyric cue at {}ms: \"{}\"", first_line.timestamp_ms, first_line.text);
            }
            println!("Total synced lyric cues: {}", lyrics.synced.len());
        }
        Err(e) => {
            println!("Note: Lyrics fetch returned: {}", e);
        }
    }

    Ok(())
}
