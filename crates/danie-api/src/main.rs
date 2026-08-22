//! Binary entry point for the danie HTTP API.

#[tokio::main]
async fn main() {
    if let Err(error) = danie_api::bootstrap().await {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
