//! Provider-independent application values and invariants.
//!
//! Nothing in this module knows about N-API, a database, a cloud provider, or
//! process configuration. Adapters can translate these values to their own
//! wire and persistence representations.

pub mod artifact;
pub mod chat;
pub mod document;
pub mod error;
pub mod iap_identity;
pub mod json;
pub mod message;
pub mod pagination;
pub mod stream;
pub mod suggestion;
pub mod user;
pub mod validation;
pub mod version;
pub mod vote;

pub use artifact::{Artifact, ArtifactKind};
pub use chat::{Chat, LifecycleState, Visibility};
pub use error::ValidationError;
pub use iap_identity::{iap_user_key, IapIdentity, IapSubject};
pub use json::JsonValue;
pub use message::{Message, MessageRole};
pub use pagination::PaginationPosition;
pub use stream::Stream;
pub use suggestion::Suggestion;
pub use user::User;
pub use validation::{validate_enum, validate_identifier, validate_payload_size};
pub use version::DocumentVersion;
pub use vote::Vote;
