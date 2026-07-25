# 04 — ChatRepository: chats and history

## Goal

Move chat creation, lookup, visibility/title updates, and history pagination to
Firestore.

## Scope

- Define the chat portion of `ChatRepository`.
- Implement root `chats/{chatId}` records with create-only semantics.
- Use an infrastructure DTO with explicit Firestore field names and validate
  the chat ID, owner, and immutable create payload on every read/write.
- Duplicate creates must compare the existing owner and immutable fields before
  returning success; a retry must not silently overwrite a different chat.
- Add a deletion state/tombstone to chats. Reads must exclude chats marked for
  deletion immediately, while physical descendant cleanup is deferred. Store
  nullable `deletedAt` and a numeric `lifecycleRevision` on the domain/DTO so
  later writes can be fenced.
- Apply field-specific updates or Firestore preconditions for title, visibility,
  and lifecycle changes. Do not use process-local locks for chat coordination.
- Implement user-filtered history with deterministic `(createdAt, document ID)`
  ordering and opaque cursors containing both values. Do not use offsets.
- Preserve `starting_after`, `ending_before`, `hasMore`, and missing-anchor
  behavior from the current API.

## Tests and checkpoint

Test duplicate creates from independent repository instances, owner/payload
conflicts, ownership filtering, both cursor directions, equal timestamps, page
boundaries, missing anchors, and concurrent title/visibility updates. Verify
that tombstoned chats disappear from history immediately and that a stale or
retrying write cannot revive a tombstone. The history route can be switched
behind a temporary direct application test, but no global Postgres removal
happens here.
