# Chatbot

Chatbot is a Rust web application built with Axum and Leptos. It runs behind
GCP Identity-Aware Proxy on Cloud Run; there is no cookie-based authentication,
CORS policy, or cross-site credential flow.

## Architecture

- `chatbot-core` contains domain types and application services.
- `chatbot-infrastructure` contains Firestore, GCS, IAP, and OpenRouter
  adapters.
- `chatbot-protocol` contains the stable HTTP and SSE DTOs.
- `chatbot-web` is the Axum server and Leptos browser client.
- `native` is a Rust-only maintenance-tools crate containing the temporary
  Open WebUI importer.

Cloud Run receives IAP-authenticated requests and the server validates the
signed IAP assertion and audience. Firestore and Google Cloud Storage are real
GCP services provisioned by Terraform; local development does not use
emulators.

## Running locally

Provision or select the Terraform `test` workspace first. The full workflow is
documented in [`terraform/README.md`](./terraform/README.md).

```bash
gcloud auth application-default login
cp terraform/backend.hcl.example terraform/backend.hcl
terraform -chdir=terraform init -backend-config=backend.hcl
terraform -chdir=terraform workspace select test
terraform -chdir=terraform apply
```

Copy the local configuration template and set the resulting `GCS_BUCKET`,
`FIRESTORE_PROJECT_ID`, and `FIRESTORE_DATABASE_ID` values along with the
OpenRouter key:

```nu
cp .env.toml.example .env.toml
```

Nushell can load that TOML file into the current process environment:

```nu
open .env.toml | from toml | load-env
```

The application itself reads environment variables, not TOML. Developers using
another shell can set the same variables directly instead of creating a file.

When using test authentication, send
`x-goog-authenticated-user-email` and `x-goog-authenticated-user-id` headers
with the `accounts.google.com:` prefix on authenticated requests.

Start the server with:

```bash
cargo run -p chatbot-web
```

Open <http://localhost:8080>. `SECRETS_GCS_PATH` may be set when testing the
GCS secret-object path used by Cloud Run.

## Verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets
cargo check -p chatbot-web --target wasm32-unknown-unknown \
  --no-default-features --features hydrate
terraform -chdir=terraform fmt -check
terraform -chdir=terraform validate
```

The workspace includes a Playwright MCP browser for manual UI verification.
Use an accessibility snapshot before interacting with a route, inspect browser
console errors, and take screenshots when layout is part of the change.

## Data invariants

Editing a message deletes the selected message and every later message in one
Firestore transaction, using the full `(createdAt, id)` position. Branches over
500 messages are rejected before any write; Firestore's 10 MiB transaction
limit also applies. The collection-scoped `messages_position` index must be
READY before deploying this path.

## Deployment

Terraform provisions Cloud Run, direct IAP, Firestore, GCS, Artifact Registry,
Cloud Build, and runtime service accounts. Cloud Build builds the Rust image
and deploys it to the workspace-specific service. See
[`terraform/README.md`](./terraform/README.md) for the one-time GCP setup,
secrets-object contract, and workspace deployment process.
