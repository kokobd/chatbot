use firestore::{FirestoreDb, FirestoreDbOptions, FirestoreResult};

/// Creates a Firestore client for an explicitly selected GCP project and
/// database. Authentication is delegated to the crate's Application Default
/// Credentials support; configuration is supplied by the composition root or
/// by a process-specific entrypoint.
pub(crate) async fn connect(project_id: &str, database_id: &str) -> FirestoreResult<FirestoreDb> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    FirestoreDb::with_options(
        FirestoreDbOptions::new(project_id.to_string()).with_database_id(database_id.to_string()),
    )
    .await
}

#[cfg(test)]
mod tests {
    use std::env;

    use firestore::{FirestoreQueryCursor, FirestoreQueryDirection};
    use serde::{Deserialize, Serialize};
    use uuid::Uuid;

    use super::connect;

    #[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
    struct CapabilityDocument {
        id: String,
        sequence: i64,
        value: String,
    }

    #[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
    struct CapabilityParent {
        id: String,
    }

    fn required_stage_environment(name: &str) -> String {
        env::var(name).unwrap_or_else(|_| {
            panic!("{name} must be configured for the Firestore capability test")
        })
    }

    fn stage_database_configuration() -> (String, String) {
        assert!(
            env::var_os("FIRESTORE_EMULATOR_HOST").is_none(),
            "FIRESTORE_EMULATOR_HOST must not be set for the real-GCP capability test"
        );

        let project_id = required_stage_environment("FIRESTORE_PROJECT_ID");
        let database_id = required_stage_environment("FIRESTORE_DATABASE_ID");
        assert_ne!(
            database_id, "(default)",
            "FIRESTORE_DATABASE_ID must identify a named Firestore database"
        );
        (project_id, database_id)
    }

    async fn delete_nested_collection(
        db: &firestore::FirestoreDb,
        collection: &str,
        parent_path: &str,
    ) -> firestore::FirestoreResult<()> {
        loop {
            let documents: Vec<CapabilityDocument> = db
                .fluent()
                .select()
                .from(collection)
                .parent(parent_path)
                .limit(25)
                .obj()
                .query()
                .await?;

            if documents.is_empty() {
                return Ok(());
            }

            let writer = db.create_simple_batch_writer().await?;
            let mut batch = writer.new_batch();
            for document in documents {
                db.fluent()
                    .delete()
                    .from(collection)
                    .parent(parent_path)
                    .document_id(&document.id)
                    .add_to_batch(&mut batch)?;
            }
            batch.write().await?;
        }
    }

    #[tokio::test]
    async fn firestore_supports_required_primitives() -> firestore::FirestoreResult<()> {
        let (project_id, database_id) = stage_database_configuration();
        let db = connect(&project_id, &database_id).await?;
        let prefix = format!("capability_{}", Uuid::new_v4().simple());
        let parents = format!("{prefix}_parents");
        let children = format!("{prefix}_children");
        let parent = CapabilityParent {
            id: "parent-1".to_string(),
        };
        let parent_path = db.parent_path(&parents, &parent.id)?.to_string();

        let result: firestore::FirestoreResult<()> = async {
            db.fluent()
                .insert()
                .into(&parents)
                .document_id(&parent.id)
                .object(&parent)
                .execute::<CapabilityParent>()
                .await?;

            let first = CapabilityDocument {
                id: "first".to_string(),
                sequence: 1,
                value: "created".to_string(),
            };
            let second = CapabilityDocument {
                id: "second".to_string(),
                sequence: 2,
                value: "created".to_string(),
            };

            for document in [&first, &second] {
                db.fluent()
                    .insert()
                    .into(&children)
                    .document_id(&document.id)
                    .parent(&parent_path)
                    .object(document)
                    .execute::<CapabilityDocument>()
                    .await?;
            }

            let loaded: Option<CapabilityDocument> = db
                .fluent()
                .select()
                .by_id_in(&children)
                .parent(&parent_path)
                .obj()
                .one(&first.id)
                .await?;
            assert_eq!(loaded, Some(first.clone()));

            let updated = CapabilityDocument {
                value: "updated".to_string(),
                ..first.clone()
            };
            db.fluent()
                .update()
                .in_col(&children)
                .document_id(&updated.id)
                .parent(&parent_path)
                .object(&updated)
                .execute::<CapabilityDocument>()
                .await?;

            let transaction_updated = CapabilityDocument {
                value: "transaction-updated".to_string(),
                ..updated.clone()
            };
            let mut transaction = db.begin_transaction().await?;
            db.fluent()
                .update()
                .in_col(&children)
                .document_id(&transaction_updated.id)
                .parent(&parent_path)
                .object(&transaction_updated)
                .add_to_transaction(&mut transaction)?;
            transaction.commit().await?;

            let after_transaction: Option<CapabilityDocument> = db
                .fluent()
                .select()
                .by_id_in(&children)
                .parent(&parent_path)
                .obj()
                .one(&transaction_updated.id)
                .await?;
            assert_eq!(after_transaction, Some(transaction_updated.clone()));

            let grouped: Vec<CapabilityDocument> = db
                .fluent()
                .select()
                .from(children.as_str())
                .all_descendants()
                .obj()
                .query()
                .await?;
            assert_eq!(grouped.len(), 2);

            let ordered: Vec<CapabilityDocument> = db
                .fluent()
                .select()
                .from(children.as_str())
                .parent(&parent_path)
                .order_by([("sequence", FirestoreQueryDirection::Ascending)])
                .obj()
                .query()
                .await?;
            assert_eq!(ordered.len(), 2);
            assert_eq!(ordered[0].sequence, 1);

            let page: Vec<CapabilityDocument> = db
                .fluent()
                .select()
                .from(children.as_str())
                .parent(&parent_path)
                .order_by([("sequence", FirestoreQueryDirection::Ascending)])
                .limit(1)
                .obj()
                .query()
                .await?;
            assert_eq!(page.len(), 1);

            let cursor = FirestoreQueryCursor::AfterValue(vec![page[0].sequence.into()]);
            let next_page: Vec<CapabilityDocument> = db
                .fluent()
                .select()
                .from(children.as_str())
                .parent(&parent_path)
                .order_by([("sequence", FirestoreQueryDirection::Ascending)])
                .start_at(cursor)
                .limit(1)
                .obj()
                .query()
                .await?;
            assert_eq!(next_page, vec![second.clone()]);

            delete_nested_collection(&db, &children, &parent_path).await?;
            let remaining: Vec<CapabilityDocument> = db
                .fluent()
                .select()
                .from(children.as_str())
                .parent(&parent_path)
                .obj()
                .query()
                .await?;
            assert!(remaining.is_empty());

            db.fluent()
                .delete()
                .from(&parents)
                .document_id(&parent.id)
                .execute()
                .await?;

            Ok(())
        }
        .await;

        let _ = delete_nested_collection(&db, &children, &parent_path).await;
        let _ = db
            .fluent()
            .delete()
            .from(&parents)
            .document_id(&parent.id)
            .execute()
            .await;
        result
    }
}
