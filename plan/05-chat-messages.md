# 05 — ChatRepository: messages and usage

## Goal

Move message persistence and the recent per-user usage query to Firestore.

## Scope

- Add `messages/{messageId}` under each chat.
- Store denormalized `userId` on messages for the collection-group usage query.
- Implement create-only batch saves, message updates, chat reads, ordered reads,
  and the internal `getMessageById({ chatId, messageId })` shape.
- Store `parts` and `attachments` as validated JSON payloads with indexing
  disabled; measure serialized size before every write and reject or externalize
  payloads before the Firestore document limit is reached.
- Implement the recent user-message count using `userId`, `role`, and
  `createdAt` only after the capability spike verifies the collection-group
  query and index. Treat it as a monitored hot path because it runs before each
  chat request; record latency and read-cost metrics and retain Redis time-bucket
  counters as the planned optimization if the benchmark is too slow or costly.

## Tests and checkpoint

Cover duplicate message IDs, message ordering, JSON round trips, payload limits,
updates, cutoff timestamps, and collection-group counts across multiple chats.
Include equal-timestamp messages, payloads near the size boundary, and a
latency/cost benchmark for the usage query.
