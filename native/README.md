# Native service architecture

The Rust package is the application composition root for server-only native
capabilities. It exposes one explicit `External<Service>` handle to Node.js.

## Responsibilities

- `application/` contains Rust-native application behavior. `FileUploadService`
  owns upload naming rules and depends on the `ObjectStorage` port trait. It
  does not know about N-API or Google Cloud.
- `infrastructure/` implements application ports. `GcsObjectStorage` owns GCS
  client construction, `GCS_BUCKET` configuration, uploads, public URL
  generation, and provider error mapping.
- `service.rs` is the composition root. `createService()` reads environment
  configuration, selects concrete implementations, constructs application
  services, injects their dependencies, and returns one top-level `Service`
  handle. `Service::new(...)` itself is dependency-only.
- `application/iap_identity.rs` defines the identity-provider port and native
  request/identity types. Infrastructure implements that port with the Google
  production provider or the explicitly selected local/test provider.
- `application/iap_authentication.rs` owns provider-independent identity
  normalization and error classification. It receives an injected
  `Arc<dyn IapIdentityProvider>` and never selects implementations or reads
  environment variables.
- `lib.rs` is the N-API adapter. It converts Node buffers, request evidence,
  and native results, accepts `&External<Service>`, and translates native
  errors into N-API errors.
- `lib/native.ts` is the server-side TypeScript bridge. It memoizes one
  service promise for the current Node.js runtime and keeps callers from
  passing native handles through application code.

Application ports are implemented by infrastructure providers. Tests can
construct `FileUploadService` with a fake `ObjectStorage` without credentials,
network access, or a real bucket. Provider-specific URL and adapter behavior
is tested separately under `infrastructure/`.

## TypeScript-to-Rust migration guideline

When moving server-side behavior from TypeScript into Rust:

1. Define a Rust-native application service and its port traits first.
2. Keep provider implementations in `infrastructure/` behind those traits.
3. Select concrete implementations only in the top-level `createService()`
   composition root.
4. Inject erased traits such as `Arc<dyn Trait>` into higher-level services.
5. Keep N-API DTOs in `lib.rs`; application code must not depend on N-API types.
6. Expose only operations on the top-level `External<Service>` to TypeScript.
7. Keep native handles private to `lib/native.ts`; application code calls its
   TypeScript wrappers instead.
8. Keep long-lived services free of request-specific and user-specific mutable
   state.
9. Test application behavior with fakes, infrastructure behavior separately,
   and the N-API/TypeScript boundary explicitly.

## Adding a server-side capability

1. Define the application service and its port trait in `application/`.
2. Implement the infrastructure adapter in `infrastructure/`.
3. Wire the concrete graph into `Service` in `service.rs`.
4. Add an N-API function in `lib.rs` that accepts `&External<Service>`.
5. Update `lib/native.ts` and the server-side caller.
6. Add application, infrastructure, and end-to-end tests.

Keep one explicit service handle per Node.js runtime. Rust must not introduce a
global singleton for shared infrastructure. Separate processes, worker
threads, serverless instances, and replicas may each have their own service.
`Service` may own reusable clients, pools, and caches, but it must not hold
request-specific or user-specific mutable state. Explicit shutdown handling
can be added when a future dependency requires graceful cleanup.

## Local development and verification

Install dependencies and configure the database, IAP, and AI environment as
described in the repository README. For uploads, set `GCS_BUCKET` and provide
Google credentials through Application Default Credentials or the standard
Google credential environment variables.

The Firestore capability spike uses the active stage's database configuration:

```bash
FIRESTORE_PROJECT_ID=... FIRESTORE_DATABASE_ID=... \
  pnpm native:firestore:test
```

The database ID must name a non-default Firestore database, and credentials
must be available through Application Default Credentials. The command runs
only the ignored real-GCP capability test; `pnpm native:test` remains fully
credential-free and does not contact Firestore.

```bash
pnpm native:build
pnpm native:test
pnpm exec tsc --noEmit
PORT=3000 pnpm exec playwright test tests/e2e/file-upload.test.ts
```

The end-to-end test intentionally fails with a prerequisite error when
`GCS_BUCKET` or `POSTGRES_URL` is missing. Playwright injects a deterministic
test IAP identity, selects a real PNG through the chat file input, checks the
upload response, fetches the public GCS URL, and verifies the attachment
preview in the UI.

If native loading fails, rebuild with `pnpm native:build` and ensure commands
run from the repository root. If service creation reports configuration or
credential errors, verify `GCS_BUCKET` and Application Default Credentials. If
the e2e test cannot authenticate, verify `IAP_AUTH_PROVIDER`, `POSTGRES_URL`,
and the local database migrations before rerunning it.
