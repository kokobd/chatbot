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
