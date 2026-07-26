# 05 — ChatRepository: messages and usage

## Goal

Move message persistence and the recent per-user usage query to Firestore.

## Scope

- Add `messages/{messageId}` under each chat.
- Define the application port over validated message values and stable
  repository errors; Firestore DTOs, provider mapping, and reconciliation stay
  in infrastructure.
- Extend the message domain/application value with authenticated `user_id` and
  store it as denormalized `userId` on every message for the collection-group
  usage query.
- Implement create-only batch saves, message updates, chat reads, ordered reads,
  and the internal `getMessageById({ chatId, messageId })` shape.
- Allocate message IDs before any retry. Duplicate IDs must compare the full
  immutable payload and return the existing message only when it matches; they
  must never overwrite a different message.
- Treat batch preparation/transaction setup, commit, and post-commit response
  conversion as separate phases. Reconcile only genuinely ambiguous batch or
  update outcomes inside the adapter. Retry only known setup/pre-commit
  failures under an operation-specific policy; never replay a transaction or
  batch after `OutcomeUnknown`. A successful reread with missing or mismatched
  state returns the original `OutcomeUnknown` unchanged, while a
  reconciliation-read error is attached to that original error.
- Store `parts` and `attachments` as validated JSON payloads with indexing
  disabled; measure the total encoded Firestore document size before every write
  and reject or externalize payloads before the document limit is reached.
- Use explicit infrastructure DTOs, field-specific updates, and lifecycle
  preconditions so retries and requests from different Cloud Run instances do
  not lose unrelated fields or write to a tombstoned chat.
- Implement the recent user-message count using `userId`, `role`, and
  `createdAt` only after the capability spike verifies the collection-group
  query and index. Treat it as a monitored hot path because it runs before each
  chat request; record latency and read-cost metrics and retain Redis time-bucket
  counters as the planned optimization if the benchmark is too slow or costly.

## Tests and checkpoint

Cover duplicate message IDs and payload mismatches, message ordering, JSON
round trips, payload limits, updates, cutoff timestamps, and collection-group
counts across multiple chats and independent repository instances. Include
equal-timestamp messages, payloads near the total size boundary, retryable
batch writes, ambiguous atomic-batch commits, known permission/precondition
failures without reconciliation, failed reconciliation reads, tombstone
interleavings, and a latency/cost benchmark for the usage query. For operations
split across batches, persist or deterministically reconstruct per-batch
progress.
