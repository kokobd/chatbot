# 03 — UserRepository vertical slice

## Goal

Move IAP user persistence to Firestore while leaving the rest of the app on its
existing database path.

## Scope

- Define `application/repository/user_repository.rs` over domain types, with
  validated `IapUser` and `Email` port values.
- Add `application/user_service.rs` with `UserService::get_or_create_iap_user`.
- Implement `FirestoreUserRepository` using a Firestore-safe deterministic key
  derived from the IAP subject, so creation is race-safe without SQL uniqueness.
- Store users through an infrastructure-owned DTO with explicit `id`, `email`,
  `iapSubject`, `createdAt`, and `updatedAt` fields. Reconstruct and validate a
  domain `User` on reads, including the document-ID/subject/user-ID binding.
- Make email synchronization an intent-specific update that writes only the
  email and server-generated `updatedAt`; never write a stale full user snapshot.
  Plan 3 defines last successful Firestore commit as the email conflict policy.
- Keep repository error categories in `application/repository`, including
  `OutcomeUnknown { retryable, ... }`, `FailedPrecondition`, `PermissionDenied`,
  `CorruptData`, and `Internal`; provider mappings must preserve stable
  categories rather than exposing provider messages as categories. The
  Firestore adapter must distinguish transaction setup, commit, and
  post-commit conversion phases. It owns reconciliation of genuinely
  ambiguous create and update outcomes, preserves the original unknown error
  when reconciliation fails, and must not reread after known permission or
  precondition failures. Do not blindly repeat a create.
- Add application fakes and pure adapter unit tests; put real-GCP adapter and
  cross-layer tests in `native/tests/firestore_user_repository.rs`.

Do not add the final N-API or TypeScript cutover yet; test the application layer
directly first.

## Checkpoint

Repeated and concurrent calls for one subject return one user, including when
the calls run on independent service instances. Email changes update only the
intended fields, unrelated subjects remain isolated, malformed persisted
identity bindings are rejected, and provider errors preserve stable category
and retryability information. No correctness behavior uses process-local
locks.

The fake must capture both initial reads before either create so conflict
recovery is tested deterministically, and must count create attempts. The
real-GCP integration tests must cover create-only conflicts, round trips, email
updates, malformed-record rejection, and concurrent calls against the
Terraform-managed named test database.
