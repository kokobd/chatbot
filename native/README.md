# Rust maintenance tools

This workspace member contains the one-off `import_openwebui` data migration
tool. Application behavior, the Axum server, Leptos client, repositories, and
provider adapters live in the workspace crates; this crate has no JavaScript
or runtime-server responsibilities.

The importer uses the Terraform-backed Firestore database and Google ADC. It
does not use an emulator:

```bash
FIRESTORE_PROJECT_ID=... FIRESTORE_DATABASE_ID=... \
  cargo run --manifest-path native/Cargo.toml --bin import_openwebui -- \
  --input .scratch/openwebui.sql.zstd \
  --target-email kokoybunny@gmail.com \
  --dry-run
```

Use `--apply` only after reviewing the dry-run result. The importer retains
source IDs and timestamps for idempotent reruns and verifies readback after an
applied migration.
