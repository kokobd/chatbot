# Chatbot infrastructure

This Terraform root manages one environment per Terraform CLI workspace. The
workspace name is intentionally open-ended: use names such as `main`, `test`,
`staging`, or `feature-abcdef` without changing this configuration. The `main`
workspace is production.

The application bucket is named
`chatbot-<workspace>-<project_id>`, and the Firestore database is named
`chatbot-<workspace>`. Each workspace receives its own set of resources.

## One-time state setup

The GCS backend bucket must exist before Terraform can initialize. Create the
shared state bucket once in the personal GCP project:

```sh
gcloud storage buckets create gs://terraform-state-default-501702 \
  --project=default-501702 \
  --location=us-central1 \
  --uniform-bucket-level-access
gcloud storage buckets update gs://terraform-state-default-501702 --versioning
```

The state bucket is private and shared by Terraform configurations across
personal projects. Use a unique `prefix` per project and repository so state
objects cannot collide.

## Initialize and deploy an environment

Authenticate Terraform with Google Application Default Credentials:

```sh
gcloud auth application-default login
cp backend.hcl.example backend.hcl
terraform init -backend-config=backend.hcl
terraform workspace new test
terraform apply
terraform output -raw bucket_name
```

For an existing environment, use `terraform workspace select test`. To create
a feature environment, use `terraform workspace new feature-abcdef` and apply
again. Each workspace has isolated remote state and a distinct bucket and
Firestore database.

Terraform provisions one shared Artifact Registry repository (`chatbot`), owned
by the `main` workspace. Every workspace's Cloud Build trigger can write to that
repository; image names include the commit SHA, so images remain distinct
without separate repositories. Terraform also provisions a workspace-specific
Cloud Build trigger, Cloud Run service, runtime service account, and direct
Cloud Run IAP policy. A workspace trigger runs only for pushes to the Git branch
with the same name: `main` deploys the `main` workspace, `test` deploys the
`test` workspace, and so on. Apply a workspace before pushing to its branch so
its trigger and service exist. Apply `main` before any other workspace so the
shared repository exists; applying the other workspaces then removes their
former workspace-specific repositories.

The primary IAP user is configured with `iap_user_email`. Additional permanent
users are configured with the versioned `iap_additional_user_emails` allowlist;
do not supply it only on the command line, or a later apply could revoke access.

Before the first apply that creates a trigger, connect the GitHub repository
`kokobd/chatbot` to the Cloud Build `us-central1` connection named `github`.
This is a one-time project-level OAuth connection and cannot be completed by
Terraform without an existing repository mapping. After connecting it, rerun:

```sh
terraform apply
```

The trigger checks the source, builds and pushes the runtime image, then deploys
it to its workspace Cloud Run service. When a build starts, it cancels any older
queued or running builds for the same branch, so only the newest started build
normally continues to deployment. Cloud Run waits for the new revision to be
Ready before the build succeeds. After a successful deployment the build moves
the image tag `deployed-<workspace>` (for example, `deployed-main`) to that
revision. Artifact Registry retains the current successfully deployed image for
each workspace indefinitely. All other images—including the reusable Docker
dependency cache, superseded deployments, and failed deployment candidates—
become eligible for deletion after one day. Cleanup is asynchronous, so an
eligible image can remain until Artifact Registry's next background run. The
image is intentionally ignored by Terraform lifecycle management, while
Terraform continues to manage the service environment variables, service
account, IAP configuration, and other infrastructure settings.

## Project-wide GCS secrets

The project-wide secrets bucket is intentionally managed outside Terraform.
Create it once with `gcloud` and keep it private:

```sh
export SECRETS_BUCKET="$(terraform output -raw secrets_bucket_name)"
gcloud storage buckets create "gs://${SECRETS_BUCKET}" \
  --project="$(terraform output -raw firestore_project_id)" \
  --location=us-central1 \
  --uniform-bucket-level-access \
  --public-access-prevention
```

Create exactly two JSON objects, each containing a map of environment-variable
names to string values. `main` reads `production.json`; every other workspace
reads `test.json`. Terraform grants each runtime service account read access to
only the object it uses.

```sh
gcloud storage cp ./production.json "gs://${SECRETS_BUCKET}/production.json"
gcloud storage cp ./test.json "gs://${SECRETS_BUCKET}/test.json"
```

The required secret in each file is its matching OpenRouter key:

```json
{ "OPENROUTER_API_KEY": "<OpenRouter key>" }
```

Secret values are loaded once by the Rust native service during eager process
startup. They are never stored in Terraform state, Cloud Build substitutions,
or the Cloud Run service specification. Update a secret object before deploying
a revision that needs it.

The `test` workspace uses Firestore Native mode in the location configured by
`firestore_location` (default `us-central1`). Apply it with:

```sh
terraform workspace select test
terraform fmt -check
terraform validate
terraform plan
terraform apply
```

The Firestore API and named database are managed by Terraform. The database is
empty when created; no application data is imported or migrated. The `main`
workspace enables point-in-time recovery and deletion protection, while other
workspaces use a destroyable database.

Set the output bucket name as `GCS_BUCKET` for the application. The bucket
allows public object reads because the application returns direct
`storage.googleapis.com` URLs for uploaded files.

Use these outputs to configure the Firestore capability test:

```sh
export FIRESTORE_PROJECT_ID="$(terraform output -raw firestore_project_id)"
export FIRESTORE_DATABASE_ID="$(terraform output -raw firestore_database_id)"
pnpm native:firestore:test
```

The capability test is part of the native test suite and requires these
variables and Google Application Default Credentials whenever native tests are
run:

```sh
export FIRESTORE_PROJECT_ID="$(terraform output -raw firestore_project_id)"
export FIRESTORE_DATABASE_ID="$(terraform output -raw firestore_database_id)"
pnpm native:test
```

Verify the named database directly when needed:

```sh
gcloud firestore databases describe --database=chatbot-test \
  --project="$(terraform output -raw firestore_project_id)"
```

## Validation

```sh
terraform fmt -check
terraform validate
terraform workspace list
terraform plan
```

Do not use the Terraform `default` workspace; this configuration rejects it to
avoid creating an accidentally unnamed environment.

## Existing manual bucket

After the Terraform-managed `test` bucket is applied and verified, remove the
old manually created bucket and its objects:

```sh
gcloud storage rm --recursive gs://chatbot-test-default-501702-20260724
```
