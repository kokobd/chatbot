# Repository guidance

## Version control

Use `jj`, not `git`, for version control operations.

## Containers

Use `podman`, not `docker`, for local container operations on this machine.
Cloud Build may use its managed Docker builder.

## Rust architecture

- Keep domain and application behavior in `crates/chatbot-core`.
- Put Google/OpenRouter implementations behind ports in
  `crates/chatbot-infrastructure`.
- Select implementations only in the top-level `chatbot-web` composition
  root.
- Keep request-specific identity and cancellation context in request scope;
  long-lived services must not own user-specific mutable state.
- Keep protocol DTOs in `crates/chatbot-protocol`, outside application code.
- Keep authentication at the IAP boundary. There is no cookie-based auth,
  CORS policy, or cross-site credential flow.

## Test environment

- Test resources are provisioned using `terraform/`.
- Local applications and tests read their configuration from inherited
  environment variables. `.env.toml` is an optional gitignored Nushell
  convenience file for loading the Terraform-created GCP resource values.
- Production secrets are managed separately through the configured GCS secret
  object.

## Browser UI verification

- Playwright MCP is available in this workspace. Use it proactively for UI
  work.
- Start or reuse the local Rust server, open the relevant route in the browser,
  and take an accessibility snapshot before choosing locators.
- Check browser console errors after interactions and take screenshots when
  visual layout is part of the change.
- Prefer deterministic mocked API routes for component and layout checks. Use
  a real model only for a small, useful smoke test and clean up temporary data.

## Message editing persistence

- Submitting an edited message deletes the selected message and every later
  message in the same chat in one Firestore transaction.
- The boundary is the full `(createdAt, id)` position, so equal timestamps are
  deterministic and the edited message is not duplicated when saved again.
- A branch with 501 or more messages is rejected before any delete is
  committed. Deletion is not chunked.
- Firestore's separate 10 MiB transaction limit still applies.
- The collection-scoped `messages_position` index (`createdAt ASC`, `id ASC`)
  must be READY before deploying this path.
- RFC3339 timestamps remain strings through the protocol boundary so
  sub-millisecond precision is preserved.
- The transaction protects its own snapshot. Writes from another tab or
  stream after it commits are outside this operation's scope.
