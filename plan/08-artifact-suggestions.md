# 08 — ArtifactRepository: suggestions

## Goal

Complete artifact persistence by moving suggestions and their cleanup to
Firestore.

## Scope

- Store suggestions under the artifact parent.
- Associate every suggestion with an explicit `versionId`, not only a document
  timestamp.
- Implement batch save and lookup by artifact ID.
- Mark suggestions associated with logically removed versions so reads hide them
  immediately. Physical deletion belongs to the nightly Cloud Run Job.
- Preserve nullable descriptions, resolution state, timestamps, and user IDs.

## Tests and checkpoint

Test JSON/string fields, multiple artifact versions, lookup isolation, batch
writes, and cleanup that removes only suggestions belonging to deleted versions.
Verify that the application process never performs unbounded suggestion garbage
collection.
