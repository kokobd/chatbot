resource "google_project_service" "firestore" {
  project            = var.project_id
  service            = "firestore.googleapis.com"
  disable_on_destroy = false
}

resource "google_firestore_database" "chatbot" {
  project                           = var.project_id
  name                              = local.firestore_database_name
  location_id                       = var.firestore_location
  type                              = "FIRESTORE_NATIVE"
  concurrency_mode                  = "OPTIMISTIC"
  app_engine_integration_mode       = "DISABLED"
  point_in_time_recovery_enablement = local.firestore_is_production ? "POINT_IN_TIME_RECOVERY_ENABLED" : "POINT_IN_TIME_RECOVERY_DISABLED"
  delete_protection_state           = local.firestore_is_production ? "DELETE_PROTECTION_ENABLED" : "DELETE_PROTECTION_DISABLED"
  deletion_policy                   = local.firestore_is_production ? "PREVENT" : "DELETE"

  depends_on = [
    google_project_service.firestore,
    terraform_data.environment_validation,
  ]
}

resource "google_firestore_index" "chats_history" {
  project     = var.project_id
  database    = google_firestore_database.chatbot.name
  collection  = "chats"
  query_scope = "COLLECTION"
  skip_wait   = true

  fields {
    field_path = "userId"
    order      = "ASCENDING"
  }

  fields {
    field_path = "createdAt"
    order      = "DESCENDING"
  }

  fields {
    field_path = "id"
    order      = "DESCENDING"
  }

  fields {
    field_path = "lifecycle"
    order      = "DESCENDING"
  }
}
