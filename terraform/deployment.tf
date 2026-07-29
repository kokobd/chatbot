data "google_project" "current" {
  project_id = var.project_id
}

resource "google_project_service" "cloudbuild" {
  project            = var.project_id
  service            = "cloudbuild.googleapis.com"
  disable_on_destroy = false
}

resource "google_project_service" "artifactregistry" {
  project            = var.project_id
  service            = "artifactregistry.googleapis.com"
  disable_on_destroy = false
}

resource "google_project_service" "run" {
  project            = var.project_id
  service            = "run.googleapis.com"
  disable_on_destroy = false
}

resource "google_project_service" "iap" {
  project            = var.project_id
  service            = "iap.googleapis.com"
  disable_on_destroy = false
}

resource "google_artifact_registry_repository" "images" {
  count = local.firestore_is_production ? 1 : 0

  project       = var.project_id
  location      = var.location
  repository_id = local.artifact_repository_name
  format        = "DOCKER"
  description   = "Shared container images for chatbot environments."
  labels = {
    application = "chatbot"
    managed_by  = "terraform"
  }

  # Keep the latest successful deployment for every stage. All other images,
  # including build caches and failed deployment candidates, expire after a day.
  cleanup_policies {
    id     = "keep-current-stage-deployments"
    action = "KEEP"

    condition {
      tag_state    = "TAGGED"
      tag_prefixes = ["deployed-"]
    }
  }

  cleanup_policies {
    id     = "delete-non-deployed-build-images"
    action = "DELETE"

    condition {
      tag_state  = "ANY"
      older_than = "86400s"
    }
  }

  cleanup_policy_dry_run = false

  depends_on = [
    google_project_service.artifactregistry,
    terraform_data.environment_validation,
  ]
}

resource "google_service_account" "cloud_build" {
  project      = var.project_id
  account_id   = local.build_service_account_id
  display_name = "Chatbot ${local.environment} Cloud Build"
}

resource "google_project_iam_member" "cloud_build_builder" {
  project = var.project_id
  role    = "roles/cloudbuild.builds.builder"
  member  = google_service_account.cloud_build.member
}

resource "google_project_iam_member" "cloud_build_log_writer" {
  project = var.project_id
  role    = "roles/logging.logWriter"
  member  = google_service_account.cloud_build.member
}

resource "google_artifact_registry_repository_iam_member" "cloud_build_writer" {
  project    = var.project_id
  location   = var.location
  repository = local.artifact_repository_name
  role       = "roles/artifactregistry.writer"
  member     = google_service_account.cloud_build.member
}

resource "google_service_account" "runtime" {
  project      = var.project_id
  account_id   = local.runtime_service_account_id
  display_name = "Chatbot ${local.environment} Cloud Run"
}

resource "google_project_iam_member" "runtime_firestore" {
  project = var.project_id
  role    = "roles/datastore.user"
  member  = google_service_account.runtime.member
}

resource "google_storage_bucket_iam_member" "uploads_runtime_creator" {
  bucket = google_storage_bucket.uploads.name
  role   = "roles/storage.objectCreator"
  member = google_service_account.runtime.member
}

resource "google_storage_bucket_iam_member" "runtime_secrets_reader" {
  bucket = local.secrets_bucket_name
  role   = "roles/storage.objectViewer"
  member = google_service_account.runtime.member

  condition {
    title       = "read-${replace(local.secrets_object_path, ".", "-")}"
    description = "Allows this runtime service account to read its selected secret object."
    expression  = "resource.name == 'projects/_/buckets/${local.secrets_bucket_name}/objects/${local.secrets_object_path}'"
  }
}

resource "google_project_iam_member" "cloud_build_run_developer" {
  project = var.project_id
  role    = "roles/run.developer"
  member  = google_service_account.cloud_build.member
}

resource "google_service_account_iam_member" "cloud_build_runtime_user" {
  service_account_id = google_service_account.runtime.name
  role               = "roles/iam.serviceAccountUser"
  member             = google_service_account.cloud_build.member
}

resource "google_cloudbuild_trigger" "workspace" {
  project         = var.project_id
  location        = var.location
  name            = local.cloud_build_trigger_name
  filename        = "cloudbuild.yaml"
  service_account = google_service_account.cloud_build.name

  repository_event_config {
    repository = local.cloud_build_repository
    push {
      branch = "^${local.environment}$"
    }
  }

  substitutions = {
    _LOCATION   = var.location
    _REPOSITORY = local.artifact_repository_name
    _SERVICE    = google_cloud_run_v2_service.chatbot.name
    _STAGE      = local.environment
  }

  depends_on = [
    google_project_service.cloudbuild,
    google_project_iam_member.cloud_build_builder,
    google_project_iam_member.cloud_build_log_writer,
    google_artifact_registry_repository_iam_member.cloud_build_writer,
    google_project_iam_member.cloud_build_run_developer,
    google_service_account_iam_member.cloud_build_runtime_user,
  ]
}

resource "google_cloud_run_v2_service" "chatbot" {
  project             = var.project_id
  name                = local.cloud_run_service_name
  location            = var.location
  ingress             = "INGRESS_TRAFFIC_ALL"
  iap_enabled         = true
  deletion_protection = local.firestore_is_production
  labels              = local.labels

  template {
    service_account = google_service_account.runtime.email

    containers {
      image = "us-docker.pkg.dev/cloudrun/container/hello"

      env {
        name  = "NODE_ENV"
        value = "production"
      }

      env {
        name  = "IAP_AUTH_PROVIDER"
        value = "google"
      }

      env {
        name  = "IAP_JWT_AUDIENCE"
        value = "/projects/${data.google_project.current.number}/locations/${var.location}/services/${local.cloud_run_service_name}"
      }

      env {
        name  = "GCS_BUCKET"
        value = google_storage_bucket.uploads.name
      }

      env {
        name  = "FIRESTORE_PROJECT_ID"
        value = var.project_id
      }

      env {
        name  = "FIRESTORE_DATABASE_ID"
        value = google_firestore_database.chatbot.name
      }

      env {
        name  = "SECRETS_GCS_PATH"
        value = local.secrets_gcs_path
      }
    }
  }

  lifecycle {
    ignore_changes = [
      client,
      client_version,
      template[0].containers[0].image,
    ]
  }

  depends_on = [
    google_project_service.run,
    google_project_service.iap,
    google_service_account.runtime,
    google_project_iam_member.runtime_firestore,
    google_storage_bucket_iam_member.uploads_runtime_creator,
    google_storage_bucket_iam_member.runtime_secrets_reader,
  ]
}

resource "google_cloud_run_v2_service_iam_member" "iap_invoker" {
  project  = var.project_id
  location = google_cloud_run_v2_service.chatbot.location
  name     = google_cloud_run_v2_service.chatbot.name
  role     = "roles/run.invoker"
  member   = "serviceAccount:service-${data.google_project.current.number}@gcp-sa-iap.iam.gserviceaccount.com"

  depends_on = [google_project_service.iap]
}

resource "google_iap_web_cloud_run_service_iam_member" "allowed_user" {
  project                = var.project_id
  location               = google_cloud_run_v2_service.chatbot.location
  cloud_run_service_name = google_cloud_run_v2_service.chatbot.name
  role                   = "roles/iap.httpsResourceAccessor"
  member                 = "user:${var.iap_user_email}"

  depends_on = [google_project_service.iap]
}

resource "google_iap_web_cloud_run_service_iam_member" "additional_allowed_user" {
  for_each = var.iap_additional_user_emails

  project                = var.project_id
  location               = google_cloud_run_v2_service.chatbot.location
  cloud_run_service_name = google_cloud_run_v2_service.chatbot.name
  role                   = "roles/iap.httpsResourceAccessor"
  member                 = "user:${each.value}"

  depends_on = [google_project_service.iap]
}
