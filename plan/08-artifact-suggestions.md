# 08 — ArtifactRepository: suggestions

## Goal

Complete artifact persistence by moving suggestions and their cleanup to
Firestore.

## Scope

- Store suggestions under the artifact parent.
- Define the application port over validated suggestion/version values and
  stable repository errors; keep Firestore DTOs, provider mapping, and
  reconciliation in infrastructure.
- Add and persist an explicit `version_id` on every suggestion (serialized as
  `versionId`), not only a document timestamp. Use infrastructure DTOs with
  explicit field names.
- Implement batch save and lookup by artifact ID.
- Allocate stable suggestion IDs before any permitted retry. Duplicate batch
  writes must compare immutable fields and be idempotent across independent
  instances.
- Separate batch preparation, transaction/setup, commit, and post-commit
  conversion phases. Reconcile only ambiguous batch outcomes inside the
  adapter. Retry only known setup/pre-commit failures under an
  operation-specific policy; never replay a transaction or batch after
  `OutcomeUnknown`. A successful reread with missing or mismatched state
  returns the original `OutcomeUnknown` unchanged, while a reconciliation-read
  error is attached to that original error.
- Mark suggestions associated with logically removed versions so reads hide them
  immediately. Physical deletion belongs to the nightly Cloud Run Job.
- Preserve nullable descriptions, resolution state, timestamps, and user IDs.

## Tests and checkpoint

Test JSON/string fields, multiple artifact versions, lookup isolation, batch
writes, duplicate-ID conflicts, and cleanup that removes only suggestions
belonging to deleted versions. Include known permission/precondition failures
without reconciliation, ambiguous atomic-batch commits, missing records, and
failed reconciliation reads. For operations split across batches, persist or
deterministically reconstruct per-batch progress. Verify that the application
process never performs unbounded suggestion garbage collection or depends on
process-local coordination.
