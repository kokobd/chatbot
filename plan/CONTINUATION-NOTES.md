# Continuation notes for Plans 3+

These notes preserve decisions and lessons from Plan 02. Read them before
starting the repository slices.

## Plan 02 result

The provider-independent domain layer is in `native/src/domain/`. It has no
Firestore, N-API, environment, or network dependencies. The public module
surface includes users, IAP identity, chats, visibility and lifecycle state,
messages and roles, votes, streams, artifacts, document versions, suggestions,
pagination positions, JSON values, validation, and persistence errors.

Important invariants:

- IDs are opaque `String` values validated as non-empty, trimmed identifiers.
- IAP subjects are trimmed, non-empty, case-sensitive, and capped at 512 bytes.
- `iap_user_key` uses UUID v5 with `Uuid::NAMESPACE_URL` and the stable name
  prefix `chatbot:iap:`. Keep later Firestore user-key derivation compatible
  with `IapIdentity::user_key()`.
- Visibility, lifecycle, message-role, and artifact-kind enums serialize in
  lowercase wire form.
- Identifier payloads are capped at 256 bytes; the default serialized JSON
  payload limit is 1 MiB.
- `PaginationPosition` orders by `(created_at, id)`, including equal-timestamp
  tie-breaking. Later cursors must preserve both values and remain opaque at
  the TypeScript boundary.
- `JsonValue` is `serde_json::Value`; nullable content fields are intentional.

The domain types are deliberately not wired into the existing SQL-backed
application yet. Plans 3–8 should add application repository traits and
services around these types, with provider implementations only in
`native/src/infrastructure`.

## Live smoke-test lessons

Playwright now uses real OpenRouter. There is no `PLAYWRIGHT=True` mock mode and
`lib/ai/models.mock.ts` was removed. The curated text smoke model is
`deepseek/deepseek-v4-flash`; it is text-only for this coverage. The combined
command is:

```sh
pnpm test:e2e:smoke
```

It runs the GCS upload smoke and DeepSeek chat smoke serially with a dedicated
test IAP identity. The DeepSeek test must wait for the complete `/api/chat`
response (`response.finished()`) before asserting the real
`message-assistant` element or deleting the generated chat. The generic
`[data-role="assistant"]` selector also matches the `Waiting...` placeholder
and can make cleanup race chat persistence.

## Verification baseline

Useful checks from Plan 02:

```sh
FIRESTORE_PROJECT_ID=<project> FIRESTORE_DATABASE_ID=<named-db> \
  cargo test --manifest-path native/Cargo.toml --lib
pnpm check
pnpm exec tsc --noEmit
terraform -chdir=terraform fmt -check -diff
terraform -chdir=terraform validate
pnpm test:e2e:smoke
```

The Firestore capability test requires a real named database and must not run
with `FIRESTORE_EMULATOR_HOST` set. Use `jj`, not `git`, for repository
operations. Keep generated native build artifacts and unrelated working-copy
changes untouched.

