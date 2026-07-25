# 01 — Firestore capability spike

## Goal

Prove that the pinned Rust `firestore` crate works with this native package and
the real named GCP database for the active stage before building application
repositories.

## Scope

- Add the exact crate version and required async/Serde features.
- Add a small infrastructure-only Firestore client constructor using project ID,
  database ID, and ADC.
- Verify named-database selection, typed CRUD operations, a transaction, a
  nested collection-group query, a cursor, and a bounded paginated batch
  delete.
- Configure the active stage with `FIRESTORE_PROJECT_ID` and
  `FIRESTORE_DATABASE_ID`; do not introduce test-specific variable names.
- Run the spike only after configuring the active stage with the required GCP
  environment variables and ADC.

## Checkpoint

The real-GCP capability test passes against the configured named database,
cleans up its unique data, and demonstrates every required Firestore
primitive. If a primitive is unsupported, resolve it here before repository
work begins.
