# 03 — UserRepository vertical slice

## Goal

Move IAP user persistence to Firestore while leaving the rest of the app on its
existing database path.

## Scope

- Define `application/repository/user_repository.rs` over domain types.
- Add `UserService::get_or_create_iap_user`.
- Implement `FirestoreUserRepository` using a Firestore-safe deterministic key
  derived from the IAP subject, so creation is race-safe without SQL uniqueness.
- Add repository fakes and real-GCP adapter tests.

Do not add the final N-API or TypeScript cutover yet; test the application layer
directly first.

## Checkpoint

Repeated and concurrent calls for one subject return one user, email changes
update the existing user, unrelated subjects remain isolated, and provider
errors map to stable application errors.
