# 07 — ArtifactRepository: document versions

## Goal

Move document creation, version retrieval, latest-version updates, and version
cleanup to Firestore.

## Scope

- Define `ArtifactRepository` over artifact and document-version domain types.
- Store `artifacts/{artifactId}` metadata and nested `versions/{versionId}`.
- Add a stable `version_id` to the document-version domain/application value
  (serialized as `versionId`) and use explicit infrastructure DTOs for artifact
  and version documents.
- Store immutable versions and maintain `headVersionId` transactionally when
  appending or manually editing a version. Never query the latest version and
  update it as two independent operations.
- Allocate version IDs before transaction attempts. Transaction retries must be
  replay-safe, and duplicate version creates must compare immutable content
  rather than overwrite it.
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
version creation from independent instances, duplicate-ID payload conflicts,
head changes, and logical timestamp-based cleanup. Verify stale transactions
cannot move the head backward.
