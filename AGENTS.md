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

