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

locals {
  environment             = terraform.workspace
  bucket_name             = "chatbot-${local.environment}-${var.project_id}"
  firestore_database_name = "chatbot-${local.environment}"
  firestore_is_production = local.environment == "prod"

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
      )
      error_message = "Use a non-default environment/workspace containing only lowercase letters, digits, and hyphens; the resulting bucket name must be at most 63 characters."
    }
  }
}
