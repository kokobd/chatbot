# Firestore migration plans

This migration is intentionally split into small vertical slices. Complete
each plan, run its checkpoint, and only then start the next one.

For durable implementation and verification details from completed work, see
[continuation notes](./CONTINUATION-NOTES.md) before starting Plans 3–11.

The architectural boundary is:

```text
domain/          pure types and invariants
application/     use cases and repository traits
infrastructure/  Firestore implementations
N-API/TS         narrow adapters at the outer boundary
```

## Distributed runtime invariants

The web service will run as multiple Cloud Run instances and revisions. No
correctness property may depend on an in-process `Mutex`, `RwLock`, cache,
singleton, or Cloud Run concurrency setting. Process-local caches are allowed
only for performance and must tolerate independent instances and eviction.

All cross-request coordination must be provided by Firestore deterministic
document IDs, transactions, write preconditions, field masks, or server-side
timestamps. Any permitted retry must be replay-safe. An ambiguous transaction
or batch must not be retried; the adapter reconciles it and returns
`OutcomeUnknown` unless the complete intended result is proven. Repository
ports expose stable application-layer errors, not Firestore types.
Infrastructure adapters classify request/setup,
commit, and post-commit conversion failures separately: only genuinely
ambiguous writes become `OutcomeUnknown`. Adapters own reconciliation of those
outcomes, attach reconciliation failures to the original unknown error, and do
not reconcile known permission, precondition, or invalid-input failures.
Application services must not repeat adapter-owned ambiguous-write
reconciliation. Persistence errors must preserve whether a failure is
retryable.

Firestore schemas belong to infrastructure DTOs with explicit wire field names
and conversions to validated domain types. Do not use provider-independent
domain serialization as the long-term Firestore schema, and validate document
identity and ownership on every read before exposing it to application code.

Repository tests must cover independent repository/service instances and
cross-instance interleavings using the real Terraform-managed test database
where practical. Test-only synchronization primitives are acceptable for
forcing races in fakes; they must not appear in production repository state.
Every repository slice must test both known-error non-reconciliation and
ambiguous-write reconciliation, including a failed reconciliation read. Batch
and transaction operations must not be blindly replayed after an ambiguous
commit; they need deterministic, operation-specific reconciliation.

Repositories follow aggregate ownership rather than SQL table count:

- `UserRepository` owns IAP user identity records.
- `ChatRepository` owns chats, messages, votes, streams, and chat deletion.
- `ArtifactRepository` owns artifact metadata, versions, and suggestions.

## Order

1. [Firestore capability spike](./01-firestore-capability-spike.md)
2. [Domain types](./02-domain-types.md)
3. [User repository](./03-user-repository.md)
4. [Chat aggregate: chats and history](./04-chat-repository.md)
5. [Chat aggregate: messages and usage](./05-chat-messages.md)
6. [Chat aggregate: votes, streams, and deletion](./06-chat-lifecycle.md)
7. [Artifact aggregate: versions](./07-artifact-repository.md)
8. [Artifact aggregate: suggestions](./08-artifact-suggestions.md)
9. [Native and TypeScript cutover](./09-native-typescript-cutover.md)
10. [Terraform, CI, and Postgres retirement](./10-infrastructure-cutover.md)
11. [Nightly Firestore garbage collection](./11-nightly-gc-job.md)

Plans 03–08 are the repository-by-repository implementation slices. Plan 09
connects the completed application capabilities to the existing N-API pattern;
Plan 10 performs the final runtime and deployment cutover. Plan 11 is a
separate Cloud Run Job and must not add background work to the Node.js process.
