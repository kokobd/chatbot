# Historical migration notes

The active implementation plan is maintained in `.scratch/plans/`. The
repository has completed the migration to a Rust workspace with Axum, Leptos,
Firestore/GCS infrastructure adapters, and a Rust-only maintenance importer.

The documents in this directory retain durable Firestore and Terraform design
decisions that are still relevant to future work. They are not build or test
instructions; use the root README for current commands.

The nightly garbage-collection job in
[`11-nightly-gc-job.md`](./11-nightly-gc-job.md) remains a separate future
Cloud Run Job and must not add background work to the web process.
