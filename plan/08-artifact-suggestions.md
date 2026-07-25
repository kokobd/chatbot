# 08 — ArtifactRepository: suggestions

## Goal

Complete artifact persistence by moving suggestions and their cleanup to
Firestore.

## Scope

- Store suggestions under the artifact parent.
- Add and persist an explicit `version_id` on every suggestion (serialized as
  `versionId`), not only a document timestamp. Use infrastructure DTOs with
  explicit field names.
- Implement batch save and lookup by artifact ID.
- Allocate stable suggestion IDs before retries. Duplicate batch writes must
  compare immutable fields and be idempotent across independent instances.
- Mark suggestions associated with logically removed versions so reads hide them
  immediately. Physical deletion belongs to the nightly Cloud Run Job.
- Preserve nullable descriptions, resolution state, timestamps, and user IDs.

## Tests and checkpoint

Test JSON/string fields, multiple artifact versions, lookup isolation, batch
writes, retryable batch replays, duplicate-ID conflicts, and cleanup that
removes only suggestions belonging to deleted versions. Verify that the
application process never performs unbounded suggestion garbage collection or
depends on process-local coordination.
