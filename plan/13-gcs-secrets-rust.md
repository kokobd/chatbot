# 13 — Rust GCS secrets and eager native initialization

## Goal

Load application secrets from a private, project-wide GCS bucket with Rust,
without adding a TypeScript secret loader or storing secret values in
Terraform, Cloud Build substitutions, or repository files.

Initialize the native service eagerly exactly once during Node/Next.js startup.
The startup path must await native service construction and secret loading
before the server is ready to handle requests; all existing native wrappers
must reuse the same cached service handle.

## Secret contract

- Terraform exposes `SECRETS_GCS_PATH` as a non-secret Cloud Run environment
  variable. The default is a workspace-specific object such as
  `gs://chatbot-secrets-<project-id>/<workspace>/app.json`.
- The bucket is project-wide and is intentionally not a Terraform resource.
  Create and manage it with `gcloud`, using uniform bucket-level access,
  public-access prevention, and no public IAM members.
- Each object is a JSON map of environment-variable names to string values,
  for example:

  ```json
  {
    "OPENROUTER_API_KEY": "...",
  }
  ```

- The Rust startup loader reads the object once per process. A configured path
  that is malformed, inaccessible, missing, or invalid JSON is a startup
  failure.
  Errors may include the bucket/object path and failure category, but never
  secret values.
- Values loaded from GCS are applied to the Node process environment by the
  startup wrapper. Existing deployment variables remain present, while keys
  supplied by the secret object take precedence. If `SECRETS_GCS_PATH` is
  unset, local development continues to use the existing environment-variable
  behavior.

## Rust implementation

- Define a Rust-native secret-source port and validated secret-map behavior.
  Keep GCS request details behind an infrastructure implementation and keep
  parsing/validation independent of the GCS client.
- Implement the GCS provider in `native/src/infrastructure`, using ADC and the
  existing Google Cloud dependency conventions. It should download one object,
  decode UTF-8, parse the JSON map, validate key/value types, and return
  application-level configuration errors.
- Load the configured secret map in the top-level Rust `create_service()`
  composition root and retain only the startup configuration needed to expose
  it through the service. `Service::new(...)` remains dependency-only and must
  not read environment variables itself.
- Add one narrow N-API operation on `External<Service>` that returns the
  validated secret map through DTOs defined in `native/src/lib.rs`. Do not add
  a standalone N-API secret handle or expose infrastructure types.
- Do not mutate the process environment directly from Rust. The N-API boundary
  returns the map to the existing TypeScript startup wrapper, which assigns
  the values to `process.env` before request handling begins.
- Keep all GCS access, path parsing, JSON validation, and redacted error
  handling in Rust. The TypeScript change is limited to startup orchestration;
  it must not add a GCS client, secret parser, or secret-specific application
  logic.

## Eager startup integration

- Change the existing native wrapper so its cached `servicePromise` is the
  single initialization path. Add an initialization function that awaits the
  service, retrieves the Rust-loaded secret map, and populates `process.env`.
- Call that function from the existing Node-only startup hook in
  `instrumentation.ts`. The hook must await initialization before completing.
  Edge execution must not import the native module.
- Ensure route handlers and database wrappers never call `createService()` a
  second time. They must reuse the already-resolved cached handle.
- If initialization fails because the configured secret object is missing,
  invalid, or unreadable, startup must fail and the service must not become
  ready. Secret values must never appear in the thrown error or logs.
- The service is initialized once per Node process/cold start. Secret changes
  take effect after a new Cloud Run instance or revision starts.

## CLI setup

Document commands to create the project-wide bucket, upload workspace JSON
objects, inspect object versions, and grant each Terraform-created runtime
service account `roles/storage.objectViewer` on the bucket. Secret files must
remain outside the repository and must never be printed by build or deploy
commands.

## Tests and checkpoint

Add Rust unit tests for GCS URI parsing, JSON validation, key precedence,
malformed values, and redacted errors. Add infrastructure tests with a fake
secret source, a real-GCS capability test that downloads a temporary
workspace object and removes it afterward, and N-API boundary tests for the
secret-map DTO.

Verify that eager startup calls native `createService()` once, all request
paths reuse the same handle, the application observes `OPENROUTER_API_KEY`
through `process.env`, missing or invalid objects prevent readiness, and
secret contents never appear in logs. Verify that the project-wide secrets
bucket is absent from Terraform state and is managed only through `gcloud`.
