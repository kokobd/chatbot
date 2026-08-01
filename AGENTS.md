# Repository guidance

## Version control

Use `jj`, not `git`, for version control operations.

## TypeScript-to-Rust migrations

For new server-side Rust capabilities:

- Define application behavior and port traits in Rust-native types.
- Put provider implementations behind those traits in `native/src/infrastructure`.
- Select implementations only in the top-level `createService()` composition root.
- Inject erased traits into higher-level application services.
- Keep `Service::new(...)` dependency-only; it must not read environment variables.
- Keep N-API DTOs in `native/src/lib.rs`, outside application code.
- Expose only operations on the top-level `External<Service>` to TypeScript.
- Keep the native handle private to `lib/native.ts`; expose narrow TypeScript wrappers.
- Do not store request-specific or user-specific mutable state in long-lived services.
- Add application tests with fakes, infrastructure tests, N-API boundary checks, and TypeScript/e2e coverage.

## Test environment

- Test environment is provisioned using terraform/
- Local .env stores the GCP resources created by terraform. LLM agents can read .env freely.
- Production secrets will be managed separately.

## Browser UI verification

- Playwright MCP is available in this workspace. Use it proactively for UI
  work; do not wait for the user to explicitly request browser testing.
- For UI changes, start or reuse the local dev server, open the relevant route
  in the Playwright browser, and verify the interaction in the rendered UI.
  Use an accessibility snapshot before choosing locators, then take screenshots
  when visual layout is part of the change.
- Check browser console errors after the interaction. Prefer deterministic
  mocked API routes for component and layout checks. Use a real model only for
  a small, clearly useful smoke test, and clean up any temporary chats or
  browser tabs afterward.
- If the dev watcher exhausts file descriptors, run it with polling enabled,
  for example:

  ```bash
  WATCHPACK_POLLING=true pnpm exec next dev --webpack -H 0.0.0.0 -p 3100
  ```

- Commands that start a GUI/browser or need network access may need to run
  outside the sandbox. Keep code edits and ordinary checks in the sandbox when
  possible.

## Message editing persistence

- Submitting an edited message deletes the selected message and every later
  message in the same chat in one Firestore transaction. The boundary is the
  full `(createdAt, id)` position, so equal timestamps are deterministic and
  the edited message is not duplicated when it is saved again.
- The transaction accepts at most 500 messages. A branch with 501 or more is
  rejected before any delete is committed; deletion is not chunked.
- Firestore's separate 10 MiB transaction limit still applies; exceeding it
  fails the transaction without partial deletes.
- The collection-scoped `messages_position` index (`createdAt ASC`, `id ASC`)
  must be READY before deploying this path. RFC3339 timestamps remain strings
  through TypeScript so sub-millisecond precision is preserved.
- The transaction protects its own snapshot. Writes from another tab or stream
  after it commits are outside this operation's scope; a revision/lock protocol
  would be a separate concurrency feature.
