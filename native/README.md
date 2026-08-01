# Native service architecture

The Rust package provides server-only capabilities for the locally running
Next.js application and for the deployed Cloud Run service. The application
process is local during development; its Firestore database and GCS storage
are remote, real GCP resources provisioned by Terraform's `test` workspace.
No Firestore or GCS emulator is used by the capability or end-to-end tests.

The local environment setup is documented in the root
[`README.md`](../README.md). The Terraform resource and workspace contract is
documented in [`terraform/README.md`](../terraform/README.md).

## Responsibilities

- `application/` contains Rust-native application behavior. `FileUploadService`
  owns upload naming rules and depends on the `ObjectStorage` port trait. It
  does not know about N-API or Google Cloud.
- `infrastructure/` implements application ports. `GcsObjectStorage` owns GCS
  client construction, `GCS_BUCKET` configuration, uploads, public URL
  generation, and provider error mapping. Firestore repositories likewise
  connect to the configured named GCP database.
- `service.rs` is the composition root. `createService()` reads environment
  configuration, selects concrete implementations, constructs application
  services, injects their dependencies, and returns one top-level `Service`
  handle. It eagerly loads the optional `SECRETS_GCS_PATH` JSON object before
  returning. `Service::new(...)` itself is dependency-only.
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
  service promise for the current Node.js runtime, initializes it from the
  Node startup hook, applies the Rust-returned secret map to `process.env`, and
  keeps callers from passing native handles through application code.

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

Run `pnpm dev` from the repository root after provisioning and selecting the
Terraform `test` workspace. The local process needs these values from that
workspace:

- `GCS_BUCKET`: the Terraform-managed upload bucket.
- `FIRESTORE_PROJECT_ID`: the GCP project containing the database.
- `FIRESTORE_DATABASE_ID`: the Terraform-managed named Firestore database.
- Google ADC: credentials for the real GCP APIs.

For local browser authentication, use `IAP_AUTH_PROVIDER=test` with
`IAP_TEST_EMAIL` and `IAP_TEST_SUBJECT`. This only substitutes the IAP identity
provider at the local boundary; it does not substitute Firestore, GCS, or
Google credentials. For uploads, `GCS_BUCKET` must point to the real bucket
and ADC must be able to access it.

The Firestore capability spike uses the active Terraform stage's database:

```bash
FIRESTORE_PROJECT_ID=... FIRESTORE_DATABASE_ID=... \
  pnpm native:firestore:test
```

The database ID must name a non-default Firestore database, credentials must
be available through ADC, and `FIRESTORE_EMULATOR_HOST` must not be set. The
capability test is included in `pnpm native:test`, so configure the same
variables before running the full native suite:

```bash
export FIRESTORE_PROJECT_ID=...
export FIRESTORE_DATABASE_ID=...
pnpm native:test
```

The application and native package can be checked with:

```bash
pnpm native:build
pnpm native:test
pnpm exec tsc --noEmit
PORT=3000 pnpm exec playwright test tests/e2e/file-upload.test.ts
```

The end-to-end upload test intentionally fails with a prerequisite error when
`GCS_BUCKET`, `FIRESTORE_PROJECT_ID`, or `FIRESTORE_DATABASE_ID` is missing.
Playwright injects a deterministic test IAP identity, selects a real PNG
through the chat file input, checks the upload response, fetches the public GCS
URL, and verifies the attachment preview in the UI.

If native loading fails, rebuild with `pnpm native:build` and ensure commands
run from the repository root. If service creation reports configuration or
credential errors, verify `GCS_BUCKET`, the Firestore project/database IDs,
and ADC. If the e2e test cannot authenticate, verify `IAP_AUTH_PROVIDER`,
`IAP_TEST_EMAIL`, and `IAP_TEST_SUBJECT` before rerunning it.

## Temporary Open WebUI import

`import_openwebui` is a local, one-off Rust binary. It is not part of the
Cloud Run service and needs only ADC with Firestore read/write access. The
target account must first sign in through IAP so the application creates its
IAP-linked user record.

```bash
FIRESTORE_PROJECT_ID=default-501702 \
FIRESTORE_DATABASE_ID=chatbot-main \
cargo run --manifest-path native/Cargo.toml --bin import_openwebui -- \
  --input .scratch/openwebui.sql.zstd \
  --target-email kokoybunny@gmail.com \
  --dry-run
```

Use `--apply` only after the dry run succeeds. The binary imports text only,
retains source IDs and timestamps for idempotent reruns, and verifies readback
after applying. Remove this binary and its `zstd` dependency after the import
has been verified in production.
