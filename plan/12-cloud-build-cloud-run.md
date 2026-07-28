# 12 — Cloud Build trigger and Cloud Run service

## Goal

Provision the application delivery infrastructure in Terraform with automatic
workspace-specific Cloud Build deployments.

Terraform must create the initial Cloud Run service with Google’s hello-world
image, manage the service configuration and environment variables, and avoid
replacing application images deployed by Cloud Build.

## Scope

- Add the required project services for Cloud Build, Artifact Registry, Cloud
  Run, and direct Cloud Run IAP.
- Create a Docker Artifact Registry repository per Terraform workspace. The
  repository name must match the workspace-specific `_REPOSITORY` substitution
  supplied to the existing `cloudbuild.yaml`.
- Create a workspace-specific Cloud Build trigger for the GitHub repository
  `kokobd/chatbot`, using `cloudbuild.yaml` and a push branch equal to the
  current workspace name.
- The trigger checks, builds, pushes, and deploys the image to its workspace
  Cloud Run service. Grant its build identity Cloud Run deployment permission
  and permission to act as that workspace's runtime service account.
- Create a workspace-specific `google_cloud_run_v2_service` with:
  - `us-docker.pkg.dev/cloudrun/container/hello` as the initial image;
  - the workspace runtime service account;
  - direct IAP enabled and non-public invocation;
  - the IAP service agent granted `roles/run.invoker`;
  - `zelin.feng99@gmail.com` granted
    `roles/iap.httpsResourceAccessor`;
  - production deletion protection consistent with the existing Firestore
    policy.
- Manage all non-secret application environment variables in the Cloud Run
  template, including `NODE_ENV`, `IAP_AUTH_PROVIDER`,
  `IAP_JWT_AUDIENCE`, `GCS_BUCKET`, `FIRESTORE_PROJECT_ID`,
  `FIRESTORE_DATABASE_ID`, and `SECRETS_GCS_PATH`.
- Create a dedicated runtime service account per workspace. Grant it only the
  application permissions required by Terraform-managed resources, including
  Firestore data access and object creation on the public-upload bucket.
  Grant it conditional object-reader access to its selected object in the
  separately managed secrets bucket.
- Ignore only the Cloud Run container image in Terraform lifecycle handling.
  Terraform must continue to own the environment-variable map, service
  account, IAP settings, and other service configuration.
- Add outputs for the service name and URI, image repository/path, trigger, the
  secret bucket/path contract, and runtime service-account email.
- Document the one-time GitHub Cloud Build connection, IAP OAuth setup for the
  external Gmail identity, workspace branch behavior, and automatic deployment.
  The deployment step must not disable IAP or replace Terraform-managed
  environment variables.

## Tests and checkpoint

Run Terraform formatting and validation, then plan at least the `test`
workspace and a second workspace. Verify that repository, trigger, service,
and runtime identity names are isolated per workspace; the trigger branch and
Cloud Build substitutions are correct; and the Cloud Run plan contains the
hello-world image, IAP configuration, and required environment variables.

After applying in a test project, verify the IAP policy and unauthenticated
request behavior, run a triggered Cloud Build, and confirm its Ready revision
uses the deployed image while a subsequent Terraform plan still detects
Terraform-owned environment-variable drift.
