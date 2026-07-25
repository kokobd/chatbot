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
