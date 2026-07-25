# Chatbot infrastructure

This Terraform root manages one environment per Terraform CLI workspace. The
workspace name is intentionally open-ended: use names such as `dev`, `test`,
`staging`, `prod`, or `feature-abcdef` without changing this configuration.

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
empty when created; no application data is imported or migrated. The `prod`
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
