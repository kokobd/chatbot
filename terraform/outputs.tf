output "bucket_name" {
  description = "The GCS bucket name to use as GCS_BUCKET."
  value       = google_storage_bucket.uploads.name
}

output "bucket_url" {
  description = "The public base URL for uploaded objects."
  value       = "https://storage.googleapis.com/${google_storage_bucket.uploads.name}"
}
