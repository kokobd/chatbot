#[tokio::main]
async fn main() -> Result<(), chatbot_web::ServerError> {
    chatbot_web::run().await
}
