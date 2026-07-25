# Chatbot infrastructure

This Terraform root manages one environment per Terraform CLI workspace. The
workspace name is intentionally open-ended: use names such as `dev`, `test`,
`staging`, `prod`, or `feature-abcdef` without changing this configuration.

The application bucket is named
`chatbot-<workspace>-<project_id>`. Cloud Run and Firestore can be added to the
same root later; each workspace will receive its own set of resources.

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
again. Each workspace has isolated remote state and a distinct bucket.

Set the output bucket name as `GCS_BUCKET` for the application. The bucket
allows public object reads because the application returns direct
`storage.googleapis.com` URLs for uploaded files.

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
