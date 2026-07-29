#[tokio::main]
async fn main() {
    if let Err(error) = chatbot_native::migration::run_cli().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
