# 09 — Native and TypeScript cutover

## Goal

Expose the completed application services through the existing native service
pattern and switch TypeScript callers without changing route behavior.

## Scope

- Inject the three repositories/services into the top-level native `Service`.
- Add typed N-API DTOs and narrow operation functions in `native/src/lib.rs`.
- Keep the native handle private to `lib/native.ts`.
- Make `lib/db/queries.ts` a compatibility adapter over native wrappers while
  retaining existing function names and response shapes.
- Replace Drizzle-derived TypeScript types with persistence-neutral domain types.
- Update the chat route where message saves now require the authenticated user
  ID.

## Tests and checkpoint

Run Rust application tests, N-API boundary tests, TypeScript type checking, and
the existing route/e2e tests against Firestore. Error categories, timestamps,
nullable values, and arbitrary JSON must survive the boundary unchanged.
