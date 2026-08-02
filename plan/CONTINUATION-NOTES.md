# Archived migration notes

The repository now uses the Rust workspace crates and the Axum/Leptos web
service. The durable design decisions from the Firestore migration remain in
the completed Rust code and tests:

- application services depend on provider-neutral repository ports;
- infrastructure adapters own provider DTO conversion and ambiguous-write
  reconciliation;
- pagination is ordered by the full `(created_at, id)` position;
- real Firestore capability tests require a named Terraform database and ADC;
- cross-instance correctness is provided by Firestore, not in-process locks.

Use the root README and the current `.scratch/plans/` files for active Rust
development and delivery commands.
