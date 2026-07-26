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
- Preserve persistence error category and retryability through the N-API and
  TypeScript compatibility adapter. Represent `OutcomeUnknown` and its
  retryability/diagnostic information without exposing Firestore error types or
  moving reconciliation into TypeScript. TypeScript must never retry
  `OutcomeUnknown`; retries are allowed only for explicitly retryable,
  non-ambiguous categories under the operation’s policy.
- Preserve Firestore timestamp precision and cursor opacity across the boundary;
  JavaScript `Date` and serialized cursors must not silently round or rebuild
  `(createdAt, id)` positions.
- Verify behavior with multiple service instances/revisions in the real test
  stage; Cloud Run concurrency settings are not a correctness mechanism.

## Tests and checkpoint

Run Rust application tests, N-API boundary tests, TypeScript type checking, and
the existing route/e2e tests against Firestore. Include retryable and
non-retryable failures, cross-instance duplicate and permitted-replay scenarios, and
timestamp/cursor round trips. Include ambiguous-write results whose
reconciliation succeeds, is missing, or fails. Error categories, retryability,
reconciliation diagnostics, timestamp precision, nullable values, and
arbitrary JSON must survive the boundary unchanged.
