variable "project_id" {
  description = "The Google Cloud project that owns the environment resources."
  type        = string
  default     = "default-501702"

  validation {
    condition     = can(regex("^[a-z][a-z0-9-]{4,28}[a-z0-9]$", var.project_id))
    error_message = "project_id must be a valid Google Cloud project ID."
  }
}

variable "location" {
  description = "The Google Cloud location for environment resources."
  type        = string
  default     = "us-central1"
}

variable "firestore_location" {
  description = "The Google Cloud location for the Firestore database."
  type        = string
  default     = "us-central1"
}

variable "force_destroy" {
  description = "Whether destroying a bucket should also delete its objects."
  type        = bool
  default     = true
}

variable "iap_user_email" {
  description = "Google account allowed to access the Cloud Run service through IAP."
  type        = string
  default     = "zelin.feng99@gmail.com"

  validation {
    condition     = can(regex("^[^@\\s]+@[^@\\s]+\\.[^@\\s]+$", var.iap_user_email))
    error_message = "iap_user_email must be a valid email address."
  }
}

variable "secrets_bucket_name" {
  description = "Project-wide GCS bucket containing application secret objects. The bucket is managed outside Terraform."
  type        = string
  default     = null

  validation {
    condition     = var.secrets_bucket_name == null || can(regex("^[a-z0-9][a-z0-9._-]{1,61}[a-z0-9]$", var.secrets_bucket_name))
    error_message = "secrets_bucket_name must be a valid GCS bucket name."
  }
}

locals {
  environment                = terraform.workspace
  bucket_name                = "chatbot-${local.environment}-${var.project_id}"
  firestore_database_name    = "chatbot-${local.environment}"
  firestore_is_production    = local.environment == "main"
  cloud_build_repository     = "projects/${var.project_id}/locations/${var.location}/connections/github/repositories/kokobd-chatbot"
  artifact_repository_name   = "chatbot"
  cloud_build_trigger_name   = "chatbot-${local.environment}-build"
  cloud_run_service_name     = "chatbot-${local.environment}"
  secrets_bucket_name        = coalesce(var.secrets_bucket_name, "chatbot-secrets-${var.project_id}")
  secrets_object_path        = local.firestore_is_production ? "production.json" : "test.json"
  secrets_gcs_path           = "gs://${local.secrets_bucket_name}/${local.secrets_object_path}"
  runtime_service_account_id = "chatbot-run-${substr(sha1(local.environment), 0, 10)}"
  build_service_account_id   = "chatbot-build-${substr(sha1(local.environment), 0, 10)}"

  labels = {
    application = "chatbot"
    environment = local.environment
    managed_by  = "terraform"
  }
}

resource "terraform_data" "environment_validation" {
  input = local.environment

  lifecycle {
    precondition {
      condition = (
        local.environment != "default"
        && can(regex("^[a-z0-9]([a-z0-9-]*[a-z0-9])?$", local.environment))
        && length(local.bucket_name) <= 63
        && length(local.cloud_run_service_name) <= 49
      )
      error_message = "Use a non-default environment/workspace containing only lowercase letters, digits, and hyphens; the resulting bucket name must be at most 63 characters and the Cloud Run service name must be at most 49 characters."
    }
  }
}
