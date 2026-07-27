output "bucket_name" {
  description = "The GCS bucket name to use as GCS_BUCKET."
  value       = google_storage_bucket.uploads.name
}

output "bucket_url" {
  description = "The public base URL for uploaded objects."
  value       = "https://storage.googleapis.com/${google_storage_bucket.uploads.name}"
}

output "firestore_project_id" {
  description = "The GCP project ID containing the workspace Firestore database."
  value       = var.project_id
}

output "firestore_database_id" {
  description = "The named Firestore database ID for the workspace."
  value       = google_firestore_database.chatbot.name
}

output "artifact_repository" {
  description = "The Artifact Registry Docker repository for this workspace."
  value       = "${var.location}-docker.pkg.dev/${var.project_id}/${google_artifact_registry_repository.images.repository_id}"
}

output "cloud_build_trigger_name" {
  description = "The Cloud Build trigger for this workspace."
  value       = google_cloudbuild_trigger.workspace.name
}

output "cloud_run_service_name" {
  description = "The Cloud Run service name for this workspace."
  value       = google_cloud_run_v2_service.chatbot.name
}

output "cloud_run_uri" {
  description = "The Cloud Run service URI for this workspace."
  value       = google_cloud_run_v2_service.chatbot.uri
}

output "runtime_service_account_email" {
  description = "The Cloud Run runtime service account email."
  value       = google_service_account.runtime.email
}

output "secrets_bucket_name" {
  description = "The project-wide GCS secrets bucket managed outside Terraform."
  value       = local.secrets_bucket_name
}

output "secrets_gcs_path" {
  description = "The workspace-specific GCS object path configured on Cloud Run."
  value       = local.secrets_gcs_path
}
