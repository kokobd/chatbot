use tokio_util::sync::CancellationToken;

/// Per-request values passed into application operations.
///
/// Long-lived services must not store this value or any user-specific mutable
/// state. The IAP subject is retained only when an operation needs it for
/// diagnostics or identity-specific behavior; authorization uses `user_id`.
#[derive(Clone, Debug)]
pub struct RequestContext {
    pub user_id: String,
    pub iap_subject: Option<String>,
    pub request_id: String,
    pub cancellation: CancellationToken,
}

impl RequestContext {
    pub fn new(user_id: String, request_id: String) -> Self {
        Self {
            user_id,
            iap_subject: None,
            request_id,
            cancellation: CancellationToken::new(),
        }
    }
}
