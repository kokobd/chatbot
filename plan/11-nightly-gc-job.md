# 11 — Nightly Firestore garbage collection job

## Status

Separate process; not part of the web request runtime. Implement after the
Firestore cutover and deploy it as a dedicated GCP Cloud Run Job scheduled
once per night.

## Goal

Physically remove Firestore descendants and obsolete artifact data that the
application has already hidden through tombstones or head-pointer changes.

## Scope

- Build a standalone job entrypoint with the shared Rust Firestore client and
  application cleanup ports/rules selected by its own composition root. Cleanup
  ports expose stable application errors and Firestore mapping remains in
  infrastructure. It must not depend on the web process.
- Purge chats marked `deleting`/`deleted`: messages, votes, streams, then the
  parent chat document.
- Purge artifact versions and suggestions marked unreachable after the
  application changes `headVersionId` or logically truncates history.
- Use a configurable grace period before physical deletion, with a safe
  development default and a longer production value.
- Process data in bounded pages and batches, persist or recompute progress from
  document state, and make every operation idempotent after interruption.
- Apply the same infrastructure write contract as the web repositories:
  distinguish setup, commit, and post-commit conversion failures; reconcile
  only ambiguous deletes or claims; preserve `OutcomeUnknown` with any failed
  reconciliation attached; and treat known not-found, permission, and
  precondition outcomes according to explicit cleanup policy rather than
  blindly retrying them.
- Never replay an ambiguous claim/delete batch. Reconcile claims by durable
  token or revision, and reconcile deletes against the complete intended
  document set.
- Assume Cloud Run Job tasks and executions can overlap or retry. Use durable
  leases/claims or disjoint deterministic work partitions when needed; never
  coordinate cleanup with in-memory locks. A lease expiry must allow recovery
  after a crashed task.
- Treat not-found deletes as successful and retain tombstones until descendant
  cleanup is verified, so a repeated or overlapping purge cannot erase the
  safety marker prematurely.
- Emit structured counts, duration, failure, and retry metrics. A failed run
  must leave tombstones intact for the next run.

## Deployment

- Provision a dedicated Cloud Run Job service account with only the required
  Firestore data-access permissions.
- Schedule the job nightly through the GCP scheduler mechanism selected by the
  infrastructure plan.
- Keep the job’s project/database configuration separate from web-process
  configuration, while targeting the same workspace database.
- Do not call the job from request handlers, `after()`, startup hooks, or
  application services.

## Tests and checkpoint

Test interrupted runs, repeated and overlapping runs, partial batches,
grace-period protection, lease recovery, not-found deletes, orphaned
subcollections, unreachable artifact versions, ambiguous batch commits,
reconciliation failures, and multi-workspace isolation against a real GCP test
database. The checkpoint is a deployable job whose retry leaves no duplicate
effects and whose normal web requests perform no physical garbage collection.
