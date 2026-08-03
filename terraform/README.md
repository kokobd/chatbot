# Chatbot GCP infrastructure

This Terraform root provisions the remote GCP resources used by Chatbot. It
does not run the application locally. For local development, provision or
select the `test` workspace here, then run only the Rust application
from the repository root. The local process connects to that workspace's real
Firestore database and GCS bucket through Google APIs; no local Firestore or
GCS emulator is expected.

## Workspace model

Terraform manages one isolated environment per CLI workspace. The workspace
name is open-ended: use names such as `test`, `main`, `staging`, or
`feature-abcdef` without changing this configuration.

- `test` is the canonical backing environment for local development.
- `main` is production.
- Other workspace names create isolated deployed environments.

Each workspace receives its own application bucket and named Firestore
database:

```text
GCS bucket:         chatbot-<workspace>-<project_id>
Firestore database: chatbot-<workspace>
```

The workspace also owns its Cloud Build trigger, Cloud Run service, runtime
service account, and direct Cloud Run IAP policy. The shared Artifact Registry
repository is owned by `main` and is used by every workspace.

## One-time state setup

The GCS backend bucket must exist before Terraform can initialize. Create the
shared state bucket once in the personal GCP project:

```bash
gcloud storage buckets create gs://terraform-state-default-501702 \
  --project=default-501702 \
  --location=us-central1 \
  --uniform-bucket-level-access
gcloud storage buckets update gs://terraform-state-default-501702 --versioning
```

The state bucket is private and shared by Terraform configurations across
personal projects. Use a unique `prefix` per project and repository so state
objects cannot collide.

## Provision the local test environment

Run these commands from the repository root. Terraform authenticates with
Google ADC and provisions remote resources; it does not launch Cloud Run or a
local application process.

```bash
gcloud auth application-default login
cp terraform/backend.hcl.example terraform/backend.hcl
terraform -chdir=terraform init -backend-config=backend.hcl
terraform -chdir=terraform workspace new test
terraform -chdir=terraform apply
```

For an existing environment:

```bash
terraform -chdir=terraform workspace select test
terraform -chdir=terraform plan
terraform -chdir=terraform apply
```

After applying, use the outputs to configure the local process:

```bash
export GCS_BUCKET="$(terraform -chdir=terraform output -raw bucket_name)"
export FIRESTORE_PROJECT_ID="$(terraform -chdir=terraform output -raw firestore_project_id)"
export FIRESTORE_DATABASE_ID="$(terraform -chdir=terraform output -raw firestore_database_id)"
```

For Nushell, copy `.env.toml.example` to `.env.toml`, put those values and the
local `OPENROUTER_API_KEY` in it, then run:

```nu
open .env.toml | from toml | load-env
cargo run -p chatbot-web
```

The application reads inherited environment variables; developers using another
shell may set the same values directly. See the root [`README.md`](../README.md)
for the complete local workflow.

Do not use the Terraform `default` workspace; this configuration rejects it to
avoid creating an accidentally unnamed environment.

## Workspace deployment resources

Terraform provisions one shared Artifact Registry repository (`chatbot`),
owned by the `main` workspace. Every workspace's Cloud Build trigger can write
to that repository; image names include the commit SHA, so images remain
distinct without separate repositories.

Apply `main` before any other workspace so the shared repository and its custom
Cloud Build role exist. Applying another workspace then creates its
workspace-specific resources. A workspace trigger runs only for pushes to the
Git branch with the same name: `main` deploys `main`, `test` deploys `test`,
and so on. Apply a workspace before pushing to its branch so its trigger and
service exist.

The trigger checks the source, builds and pushes the runtime image, then
deploys it to the workspace Cloud Run service. When a build starts, it cancels
older queued or running builds for the same branch. Cloud Run waits for the
new revision to be Ready before the build succeeds. After a successful
deployment, the build moves the image tag `deployed-<workspace>` (for example,
`deployed-main`) to that revision.

Artifact Registry retains the current successfully deployed image for each
workspace indefinitely. Other images—including the reusable Docker dependency
cache, superseded deployments, and failed deployment candidates—become
eligible for deletion after one day. Cleanup is asynchronous. The image is
ignored by Terraform lifecycle management, while Terraform continues to
manage service environment variables, service accounts, IAP configuration,
and other infrastructure settings.

The primary IAP user is configured with `iap_user_email`. Additional permanent
users are configured with the versioned `iap_additional_user_emails`
allowlist; do not supply them only on the command line, or a later apply could
revoke access.

Before the first apply that creates a trigger, connect the GitHub repository
`kokobd/chatbot` to the Cloud Build `us-central1` connection named `github`.
This one-time project-level OAuth connection cannot be completed by Terraform
without an existing repository mapping. After connecting it, rerun:

```bash
terraform -chdir=terraform apply
```

## Project-wide GCS secrets

The project-wide secrets bucket is intentionally managed outside Terraform.
Create it once with `gcloud` and keep it private:

```bash
export SECRETS_BUCKET="$(terraform -chdir=terraform output -raw secrets_bucket_name)"
gcloud storage buckets create "gs://${SECRETS_BUCKET}" \
  --project="$(terraform -chdir=terraform output -raw firestore_project_id)" \
  --location=us-central1 \
  --uniform-bucket-level-access \
  --public-access-prevention
```

Create exactly two JSON objects, each containing a map of environment-variable
names to string values. `main` reads `production.json`; every other workspace
reads `test.json`. Terraform grants each runtime service account read access to
only the object it uses.

```bash
gcloud storage cp ./production.json "gs://${SECRETS_BUCKET}/production.json"
gcloud storage cp ./test.json "gs://${SECRETS_BUCKET}/test.json"
```

The required secret in each file is its matching OpenRouter key:

```json
{ "OPENROUTER_API_KEY": "<OpenRouter key>" }
```

Secret values are loaded once by the Rust native service during eager process
startup. They are never stored in Terraform state, Cloud Build substitutions,
or the Cloud Run service specification. Update a secret object before
deploying a revision that needs it.

## Firestore and application outputs

The `test` workspace uses Firestore Native mode in the location configured by
`firestore_location` (default `us-central1`). Apply it with:

```bash
terraform -chdir=terraform workspace select test
terraform -chdir=terraform fmt -check
terraform -chdir=terraform validate
terraform -chdir=terraform plan
terraform -chdir=terraform apply
```

The Firestore API and named database are managed by Terraform. The database is
empty when created; no application data is imported or migrated. The `main`
workspace enables point-in-time recovery and deletion protection, while other
workspaces use a destroyable database.

Use these outputs to configure the local Firestore capability test:

```bash
export FIRESTORE_PROJECT_ID="$(terraform -chdir=terraform output -raw firestore_project_id)"
export FIRESTORE_DATABASE_ID="$(terraform -chdir=terraform output -raw firestore_database_id)"
cargo test -p chatbot-infrastructure --lib infrastructure::firestore::tests::firestore_supports_required_primitives -- --ignored --nocapture
```

The capability test is part of the native test suite and requires these
variables and Google ADC whenever native tests are run:

```bash
export FIRESTORE_PROJECT_ID="$(terraform -chdir=terraform output -raw firestore_project_id)"
export FIRESTORE_DATABASE_ID="$(terraform -chdir=terraform output -raw firestore_database_id)"
cargo test --workspace --all-targets
```

Verify the named database directly when needed:

```bash
gcloud firestore databases describe --database=chatbot-test \
  --project="$(terraform -chdir=terraform output -raw firestore_project_id)"
```

Set the output bucket name as `GCS_BUCKET` for the application. The bucket
allows public object reads because the application returns direct
`storage.googleapis.com` URLs for uploaded files.

## Validation

```bash
terraform -chdir=terraform fmt -check
terraform -chdir=terraform validate
terraform -chdir=terraform workspace list
terraform -chdir=terraform plan
```

## Existing manual bucket

After the Terraform-managed `test` bucket is applied and verified, remove the
old manually created bucket and its objects:

```bash
gcloud storage rm --recursive gs://chatbot-test-default-501702-20260724
```
