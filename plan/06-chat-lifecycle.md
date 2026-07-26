# 06 — ChatRepository: votes, streams, and deletion

## Goal

Complete the chat aggregate, including lifecycle cleanup.

## Scope

- Implement deterministic vote documents and validated vote upserts.
- Keep lifecycle ports and stable persistence errors in the application layer;
  keep Firestore DTOs, provider mapping, and write reconciliation in the
  infrastructure adapter.
- Implement stream creation and ordered stream lookup.
- Implement idempotent chat deletion and delete-all-by-user as tombstone
  operations. The request must mark chats as deleting/deleted and return without
  recursively purging unbounded descendants.
- Fence descendant writes against the root chat lifecycle revision using
  Firestore transactions or preconditions. An in-flight request from another
  Cloud Run instance must not create messages, votes, or streams after a chat
  is tombstoned.
- Classify transaction setup, commit, and post-commit conversion separately.
  Reconcile only `OutcomeUnknown` transaction or batch outcomes; return known
  lifecycle precondition, permission, and validation failures without a
  reconciliation read, and attach any reconciliation-read failure to the
  original unknown outcome.
- Keep all reads excluding tombstoned chats. Physical deletion of messages,
  votes, streams, and chat parents belongs exclusively to the separate nightly
  Cloud Run Job in plan 11—not to Node.js, `after()`, or a request callback.
- Make the future purge resumable, paginated, bounded by operation count and
  serialized request size, and safe to retry after partial failure. Repeated
  deletion must preserve the original tombstone timestamp and state.
- Implement timestamp-based message deletion and its vote cleanup.

## Tests and checkpoint

Test vote replacement, missing targets, stream ordering, empty deletes, partial
cleanup progress, bounded batches over Firestore’s write limit, and delete-all isolation
between users. Interleave descendant writes and tombstoning from independent
repository instances. Verify tombstoned chats disappear immediately, repeated
deletes are idempotent, and the request path performs no descendant garbage
collection. Include ambiguous transaction/batch commits, missing
reconciliation records, failed reconciliation reads, and known precondition
failures that do not reconcile. At this point all chat behavior has an
application-level fake and a real-GCP adapter test.
