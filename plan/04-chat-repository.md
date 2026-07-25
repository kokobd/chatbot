# 04 — ChatRepository: chats and history

## Goal

Move chat creation, lookup, visibility/title updates, and history pagination to
Firestore.

## Scope

- Define the chat portion of `ChatRepository`.
- Implement root `chats/{chatId}` records with create-only semantics.
- Add a deletion state/tombstone to chats. Reads must exclude chats marked for
  deletion immediately, while physical descendant cleanup is deferred.
- Implement user-filtered history with deterministic `(createdAt, document ID)`
  ordering and opaque cursors containing both values. Do not use offsets.
- Preserve `starting_after`, `ending_before`, `hasMore`, and missing-anchor
  behavior from the current API.

## Tests and checkpoint

Test duplicate creates, ownership filtering, both cursor directions, equal
timestamps, page boundaries, missing anchors, and title/visibility updates.
Verify that tombstoned chats disappear from history immediately. The history
route can be switched behind a temporary direct application test, but no global
Postgres removal happens here.
