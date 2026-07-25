# 02 — Domain types and invariants

## Goal

Create provider-independent Rust domain objects without Firestore, N-API, or
environment-variable dependencies.

## Scope

Add `native/src/domain/` modules for:

- users and IAP identity;
- chats, visibility, messages, votes, and streams;
- artifacts, document versions, and suggestions;
- pagination cursors. Repository error categories belong to the application
  repository ports; the domain exports validation errors only.

Define the invariants needed by later repositories: valid visibility/kind/role,
stable user key derivation from an IAP subject, deterministic vote identity,
document version ordering, and Firestore payload-size validation boundaries.

## Tests and checkpoint

Unit-test every invariant, equal-timestamp ordering, nullable fields, arbitrary
JSON values, and stable user-key generation. No Firestore or N-API code should
be required to compile or test these modules.
