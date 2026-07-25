# 10 — Terraform, CI, and Postgres retirement

## Goal

Make Firestore the only runtime database and remove obsolete Postgres setup.

## Scope

- Provision `firestore.googleapis.com`, a named workspace database, required
  composite indexes, and single-field exemptions.
- Add project/database outputs, runtime IAM documentation or conditional
  `roles/datastore.user` wiring, production protection/PITR, and test cleanup
  settings.
- Replace `POSTGRES_URL` with Firestore project/database configuration and ADC
  instructions in `.env.example`, README files, and CI.
- Remove Drizzle/Postgres dependencies, SQL migrations, migration scripts, and
  the migration step from the build.
- Make real-GCP integration/e2e tests explicit and configure CI authentication
  through Workload Identity Federation.
- Document the Cloud Run distributed-runtime contract: reuse a Firestore client
  per process instance for performance, but assume multiple instances and
  revisions coexist; correctness must come from Firestore transactions,
  preconditions, deterministic IDs, and idempotent retries, never process-local
  locks or concurrency limits.
- Configure request deadlines, retry/ambiguous-write metrics, and alerting for
  Firestore unavailable, conflict, and failed-precondition categories.
- Document that runtime deletion only writes tombstones. It must not start
  recursive cleanup in the Node.js process.
- Provision or document the separate Cloud Run Job identity and Firestore IAM
  needed by plan 11 without coupling that job to the web service lifecycle.

## Tests and checkpoint

Run Terraform format/validate/plan against the test and production workspaces,
native tests with the `.env`-configured named test database, TypeScript checks,
and the full Firestore-backed Playwright suite. Include rolling-revision and
retry/replay checks. Search the repository to confirm no runtime Postgres or
Drizzle references remain, no correctness lock is process-local, and no Node.js
request path owns garbage-collection work.
