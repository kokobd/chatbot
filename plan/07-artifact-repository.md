# 07 — ArtifactRepository: document versions

## Goal

Move document creation, version retrieval, latest-version updates, and version
cleanup to Firestore.

## Scope

- Define `ArtifactRepository` over artifact and document-version domain types.
- Store `artifacts/{artifactId}` metadata and nested `versions/{versionId}`.
- Store immutable versions and maintain `headVersionId` transactionally when
  appending or manually editing a version. Never query the latest version and
  update it as two independent operations.
- Preserve ascending history, latest lookup, ownership fields, and deletion of
  versions after a timestamp by marking unreachable versions for cleanup. Reads
  must hide marked versions immediately; physical cleanup belongs to the
  separate nightly Cloud Run Job in plan 11.
- Enforce the chosen Firestore payload-size boundary for document content. If
  large-content support is enabled, store a typed GCS pointer, byte size, and
  checksum instead of an oversized Firestore field.

## Tests and checkpoint

Cover first-version creation, multiple versions, latest updates, equal-time
versions, ownership data, not-found behavior, transaction retries, concurrent
version creation, head changes, and logical timestamp-based cleanup.
