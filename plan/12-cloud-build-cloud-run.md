# 12 — Cloud Build trigger and Cloud Run service

## Goal

Provision the application delivery infrastructure in Terraform while keeping
container image deployments under explicit `gcloud` CLI control.

Terraform must create the initial Cloud Run service with Google’s hello-world
image, manage the service configuration and environment variables, and avoid
replacing later application images deployed by `gcloud run deploy`.

## Scope

- Add the required project services for Cloud Build, Artifact Registry, Cloud
  Run, and direct Cloud Run IAP.
- Create a Docker Artifact Registry repository per Terraform workspace. The
  repository name must match the workspace-specific `_REPOSITORY` substitution
  supplied to the existing `cloudbuild.yaml`.
- Create a workspace-specific Cloud Build trigger for the GitHub repository
  `kokobd/chatbot`, using `cloudbuild.yaml` and a push branch that defaults to
  the current workspace. Support an explicit branch variable for workspaces
  whose branch name differs from the workspace name.
- The trigger builds and pushes the image only. It must not deploy Cloud Run;
  application deployments remain an explicit `gcloud run deploy --image ...`
  operation.
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
  Output its email so the separately managed secrets bucket can grant it
  object-reader access with `gcloud`.
- Ignore only the Cloud Run container image in Terraform lifecycle handling.
  Terraform must continue to own the environment-variable map, service
  account, IAP settings, and other service configuration.
- Add outputs for the service name and URI, image repository/path, trigger, the
  secret bucket/path contract, and runtime service-account email.
- Document the one-time GitHub Cloud Build connection, IAP OAuth setup for the
  external Gmail identity, workspace branch behavior, and the exact `gcloud`
  image-deployment command. The deployment command must not disable IAP or
  replace Terraform-managed environment variables.

## Tests and checkpoint

Run Terraform formatting and validation, then plan at least the `test`
workspace and a second workspace. Verify that repository, trigger, service,
and runtime identity names are isolated per workspace; the trigger branch and
Cloud Build substitutions are correct; and the Cloud Run plan contains the
hello-world image, IAP configuration, and required environment variables.

After applying in a test project, verify the IAP policy and unauthenticated
request behavior, run a triggered Cloud Build, deploy a custom image with
`gcloud`, and confirm a subsequent Terraform plan does not revert that image
while still detecting Terraform-owned environment-variable drift.

