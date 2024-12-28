use clap::Parser;
use spotify_sort::Spotify;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    spotify_auth: String,
    playlist_code: String,
    offset: usize,
    limit: usize,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    Spotify::new(cli.spotify_auth, None)
        .sort(cli.playlist_code, cli.offset, cli.limit)
        .await;
    Ok(())
}
