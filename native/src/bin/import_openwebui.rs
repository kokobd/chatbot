#[tokio::main]
async fn main() {
    if let Err(error) = chatbot_tools::migration::run_cli().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
