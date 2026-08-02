#[cfg(not(feature = "hydrate"))]
#[tokio::main]
async fn main() -> Result<(), chatbot_web::ServerError> {
    chatbot_web::run().await
}

#[cfg(feature = "hydrate")]
fn main() {}
