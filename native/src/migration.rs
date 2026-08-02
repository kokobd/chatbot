//! One-off import helpers. This module is deliberately separate from request
//! handling and is called only by the temporary `import_openwebui` binary.

use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufRead, BufReader},
    path::PathBuf,
    sync::Arc,
};

use chrono::{TimeZone, Utc};
use serde_json::{json, Value};
use thiserror::Error;

use crate::{
    application::{
        chat_service::ChatService,
        message_service::MessageService,
        repository::{ChatRepository, Email, MessageRepository},
    },
    domain::{Chat, Message, MessageRole, Visibility},
    infrastructure::{
        firestore::connect, firestore_chat_repository::FirestoreChatRepository,
        firestore_message_repository::FirestoreMessageRepository,
        firestore_user_repository::FirestoreUserRepository,
    },
};

const DEFAULT_SOURCE_EMAIL: &str = "kokoybunny@gmail.com";

#[derive(Debug, Clone)]
pub struct ImportOptions {
    pub input: PathBuf,
    pub source_email: String,
    pub target_email: String,
    pub apply: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSummary {
    pub chat_count: usize,
    pub message_count: usize,
    pub target_user_id: String,
    pub applied: bool,
}

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("invalid command line: {0}")]
    Cli(String),
    #[error("I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid Open WebUI backup: {0}")]
    Backup(String),
    #[error("Firestore configuration failed: {0}")]
    Firestore(String),
    #[error("migration failed: {0}")]
    Migration(String),
}

#[derive(Default)]
struct BackupData {
    source_user_id: Option<String>,
    chats: BTreeMap<String, SourceChat>,
    messages: Vec<SourceMessage>,
}

struct SourceChat {
    user_id: String,
    title: String,
    created_at: i64,
}

struct SourceMessage {
    id: String,
    chat_id: String,
    user_id: String,
    role: String,
    content: String,
    created_at: i64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CopySection {
    None,
    User,
    Chat,
    ChatMessage,
}

pub async fn run_cli() -> Result<(), ImportError> {
    let options = parse_cli(std::env::args().skip(1))?;
    let summary = run(options).await?;
    println!(
        "{}: {} chats and {} messages for user {}{}",
        if summary.applied {
            "imported"
        } else {
            "validated"
        },
        summary.chat_count,
        summary.message_count,
        summary.target_user_id,
        if summary.applied {
            " (readback verified)"
        } else {
            " (dry run)"
        },
    );
    Ok(())
}

pub async fn run(options: ImportOptions) -> Result<ImportSummary, ImportError> {
    let backup = read_backup(&options.input, &options.source_email)?;

    let project_id = required_environment("FIRESTORE_PROJECT_ID")?;
    let database_id = required_environment("FIRESTORE_DATABASE_ID")?;
    if database_id == "(default)" {
        return Err(ImportError::Firestore(
            "FIRESTORE_DATABASE_ID must identify a named Firestore database".to_string(),
        ));
    }
    let db = connect(&project_id, &database_id)
        .await
        .map_err(|error| ImportError::Firestore(error.to_string()))?;
    let target_email = Email::new(&options.target_email)
        .map_err(|error| ImportError::Migration(error.to_string()))?;
    let users = FirestoreUserRepository::new(db.clone());
    let target_ids = users
        .find_user_ids_by_email(&target_email)
        .await
        .map_err(|error| ImportError::Firestore(error.to_string()))?;
    let [target_user_id] = target_ids.as_slice() else {
        return Err(ImportError::Migration(format!(
            "expected exactly one existing IAP user for {}; found {}. The account must sign in to production before importing",
            target_email.as_str(),
            target_ids.len()
        )));
    };

    let chats_repository: Arc<dyn ChatRepository> =
        Arc::new(FirestoreChatRepository::new(db.clone()));
    let messages_repository: Arc<dyn MessageRepository> =
        Arc::new(FirestoreMessageRepository::new(db));
    let chats_service = ChatService::new(chats_repository);
    let messages_service = MessageService::new(messages_repository);
    let target_user_id = target_user_id.clone();
    let (chats, messages) = build_records(backup, &target_user_id)?;

    if options.apply {
        for chat in &chats {
            chats_service
                .create_chat(chat)
                .await
                .map_err(|error| ImportError::Migration(error.to_string()))?;
            let chat_messages = messages.get(&chat.id).ok_or_else(|| {
                ImportError::Backup(format!("chat {} has no indexed message group", chat.id))
            })?;
            messages_service
                .save_messages(chat_messages)
                .await
                .map_err(|error| ImportError::Migration(error.to_string()))?;
        }
        verify_import(
            &chats_service,
            &messages_service,
            &chats,
            &messages,
            &target_user_id,
        )
        .await?;
    }

    Ok(ImportSummary {
        chat_count: chats.len(),
        message_count: messages.values().map(Vec::len).sum(),
        target_user_id,
        applied: options.apply,
    })
}

fn parse_cli(arguments: impl IntoIterator<Item = String>) -> Result<ImportOptions, ImportError> {
    let mut input = None;
    let mut source_email = DEFAULT_SOURCE_EMAIL.to_string();
    let mut target_email = None;
    let mut apply = false;
    let mut arguments = arguments.into_iter();

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--input" => input = Some(PathBuf::from(required_argument(&mut arguments, "--input")?)),
            "--source-email" => source_email = required_argument(&mut arguments, "--source-email")?,
            "--target-email" => {
                target_email = Some(required_argument(&mut arguments, "--target-email")?)
            }
            "--apply" => apply = true,
            "--dry-run" => apply = false,
            "--help" | "-h" => {
                return Err(ImportError::Cli(
                    "usage: import_openwebui --input BACKUP.zstd --target-email EMAIL [--source-email EMAIL] [--dry-run|--apply]".to_string(),
                ));
            }
            value => return Err(ImportError::Cli(format!("unknown argument {value}"))),
        }
    }

    Ok(ImportOptions {
        input: input.ok_or_else(|| ImportError::Cli("--input is required".to_string()))?,
        source_email,
        target_email: target_email
            .ok_or_else(|| ImportError::Cli("--target-email is required".to_string()))?,
        apply,
    })
}

fn required_argument(
    arguments: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<String, ImportError> {
    arguments
        .next()
        .filter(|value| !value.starts_with("--"))
        .ok_or_else(|| ImportError::Cli(format!("{flag} requires a value")))
}

fn required_environment(name: &str) -> Result<String, ImportError> {
    std::env::var(name).map_err(|_| ImportError::Firestore(format!("{name} must be configured")))
}

fn read_backup(path: &PathBuf, source_email: &str) -> Result<BackupData, ImportError> {
    let file = File::open(path)?;
    let decoder = zstd::stream::read::Decoder::new(file)?;
    parse_backup(BufReader::new(decoder), source_email)
}

fn parse_backup(reader: impl BufRead, source_email: &str) -> Result<BackupData, ImportError> {
    let mut data = BackupData::default();
    let mut section = CopySection::None;

    for line in reader.lines() {
        let line = line?;
        section = match line.as_str() {
            value if value.starts_with("COPY public.\"user\" ") => CopySection::User,
            value if value.starts_with("COPY public.chat ") => CopySection::Chat,
            value if value.starts_with("COPY public.chat_message ") => CopySection::ChatMessage,
            "\\." => CopySection::None,
            _ => section,
        };
        if matches!(section, CopySection::None) || line.starts_with("COPY ") || line == "\\." {
            continue;
        }

        let fields = decode_copy_row(&line)?;
        match section {
            CopySection::User if required_field(&fields, 2, "user email")? == source_email => {
                let id = required_field(&fields, 0, "user ID")?.to_string();
                if data.source_user_id.replace(id).is_some() {
                    return Err(ImportError::Backup(format!(
                        "multiple source users have email {source_email}"
                    )));
                }
            }
            CopySection::Chat => {
                let id = required_field(&fields, 0, "chat ID")?.to_string();
                let chat = SourceChat {
                    user_id: required_field(&fields, 1, "chat user ID")?.to_string(),
                    title: required_field(&fields, 2, "chat title")?.to_string(),
                    created_at: parse_seconds(
                        required_field(&fields, 4, "chat created_at")?,
                        "chat",
                    )?,
                };
                if data.chats.insert(id.clone(), chat).is_some() {
                    return Err(ImportError::Backup(format!("duplicate chat ID {id}")));
                }
            }
            CopySection::ChatMessage => {
                let content = required_field(&fields, 5, "message content")?;
                let content: Value = serde_json::from_str(content).map_err(|error| {
                    ImportError::Backup(format!("message content is not JSON: {error}"))
                })?;
                let Value::String(content) = content else {
                    return Err(ImportError::Backup(
                        "message content must be a JSON string".to_string(),
                    ));
                };
                data.messages.push(SourceMessage {
                    id: required_field(&fields, 0, "message ID")?.to_string(),
                    chat_id: required_field(&fields, 1, "message chat ID")?.to_string(),
                    user_id: required_field(&fields, 2, "message user ID")?.to_string(),
                    role: required_field(&fields, 3, "message role")?.to_string(),
                    content,
                    created_at: parse_seconds(
                        required_field(&fields, 15, "message created_at")?,
                        "message",
                    )?,
                });
            }
            _ => {}
        }
    }

    let source_user_id = data
        .source_user_id
        .as_ref()
        .ok_or_else(|| ImportError::Backup(format!("source user {source_email} was not found")))?;
    data.chats.retain(|_, chat| chat.user_id == *source_user_id);
    if data.chats.is_empty() {
        return Err(ImportError::Backup("source user has no chats".to_string()));
    }
    Ok(data)
}

#[allow(clippy::type_complexity)]
fn build_records(
    backup: BackupData,
    target_user_id: &str,
) -> Result<(Vec<Chat>, BTreeMap<String, Vec<Message>>), ImportError> {
    let source_user_id = backup.source_user_id.ok_or_else(|| {
        ImportError::Backup("source user was not resolved before conversion".to_string())
    })?;
    let mut chats = Vec::with_capacity(backup.chats.len());
    for (id, chat) in backup.chats {
        reject_document_id(&id, "chat")?;
        chats.push(
            Chat::new(
                &id,
                target_user_id,
                chat.title,
                Visibility::Private,
                timestamp(chat.created_at, "chat")?,
            )
            .map_err(|error| ImportError::Backup(error.to_string()))?,
        );
    }

    let mut messages = BTreeMap::<String, Vec<Message>>::new();
    for source in backup.messages {
        if source.user_id != source_user_id {
            continue;
        }
        if !chats.iter().any(|chat| chat.id == source.chat_id) {
            return Err(ImportError::Backup(format!(
                "message {} references a chat not owned by the source user",
                source.id
            )));
        }
        reject_document_id(&source.id, "message")?;
        let role = MessageRole::parse(&source.role)
            .map_err(|error| ImportError::Backup(error.to_string()))?;
        let message = Message::new(
            source.id,
            source.chat_id.clone(),
            target_user_id,
            role,
            json!([{ "type": "text", "text": source.content }]),
            json!([]),
            timestamp(source.created_at, "message")?,
        )
        .map_err(|error| ImportError::Backup(error.to_string()))?;
        messages.entry(source.chat_id).or_default().push(message);
    }
    if messages.values().map(Vec::len).sum::<usize>() == 0 {
        return Err(ImportError::Backup(
            "source user has no messages".to_string(),
        ));
    }
    for messages in messages.values_mut() {
        messages.sort_by_key(|message| (message.created_at, message.id.clone()));
    }
    Ok((chats, messages))
}

async fn verify_import(
    chats_service: &ChatService,
    messages_service: &MessageService,
    expected_chats: &[Chat],
    expected_messages: &BTreeMap<String, Vec<Message>>,
    target_user_id: &str,
) -> Result<(), ImportError> {
    for expected_chat in expected_chats {
        let expected_chat = expected_chat.clone();
        let actual_chat = chats_service
            .find_chat(target_user_id, &expected_chat.id)
            .await
            .map_err(|error| ImportError::Migration(error.to_string()))?
            .ok_or_else(|| {
                ImportError::Migration(format!("chat {} was not found", expected_chat.id))
            })?;
        if actual_chat != expected_chat {
            return Err(ImportError::Migration(format!(
                "chat {} does not match the intended import",
                expected_chat.id
            )));
        }
        let expected = expected_messages
            .get(&expected_chat.id)
            .cloned()
            .ok_or_else(|| ImportError::Migration("missing expected messages".to_string()))?;
        let actual = messages_service
            .get_messages_by_chat_id(target_user_id, &expected_chat.id)
            .await
            .map_err(|error| ImportError::Migration(error.to_string()))?;
        if actual != expected {
            return Err(ImportError::Migration(format!(
                "messages for chat {} do not match the intended import",
                expected_chat.id
            )));
        }
    }
    Ok(())
}

fn decode_copy_row(line: &str) -> Result<Vec<Option<String>>, ImportError> {
    line.split('\t').map(decode_copy_field).collect()
}

fn decode_copy_field(value: &str) -> Result<Option<String>, ImportError> {
    if value == "\\N" {
        return Ok(None);
    }
    let mut decoded = String::with_capacity(value.len());
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '\\' {
            decoded.push(character);
            continue;
        }
        let Some(escape) = characters.next() else {
            return Err(ImportError::Backup("trailing COPY escape".to_string()));
        };
        let decoded_character = match escape {
            'b' => '\u{0008}',
            'f' => '\u{000C}',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            'v' => '\u{000B}',
            '\\' => '\\',
            digit @ '0'..='7' => {
                let mut octal = digit.to_string();
                for _ in 0..2 {
                    if matches!(characters.peek(), Some('0'..='7')) {
                        octal.push(characters.next().expect("peeked character must exist"));
                    }
                }
                let value = u8::from_str_radix(&octal, 8).map_err(|error| {
                    ImportError::Backup(format!("invalid COPY octal escape: {error}"))
                })?;
                decoded.push(char::from(value));
                continue;
            }
            value => {
                return Err(ImportError::Backup(format!(
                    "unsupported COPY escape \\{value}"
                )))
            }
        };
        decoded.push(decoded_character);
    }
    Ok(Some(decoded))
}

fn required_field<'a>(
    fields: &'a [Option<String>],
    index: usize,
    name: &str,
) -> Result<&'a str, ImportError> {
    fields
        .get(index)
        .and_then(Option::as_deref)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ImportError::Backup(format!("{name} is missing")))
}

fn parse_seconds(value: &str, kind: &str) -> Result<i64, ImportError> {
    value
        .parse()
        .map_err(|error| ImportError::Backup(format!("invalid {kind} timestamp: {error}")))
}

fn timestamp(seconds: i64, kind: &str) -> Result<chrono::DateTime<Utc>, ImportError> {
    Utc.timestamp_opt(seconds, 0)
        .single()
        .ok_or_else(|| ImportError::Backup(format!("{kind} timestamp is out of range")))
}

fn reject_document_id(id: &str, kind: &str) -> Result<(), ImportError> {
    if id.contains('/') {
        return Err(ImportError::Backup(format!(
            "{kind} ID may not contain '/'"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{build_records, decode_copy_field, parse_backup};

    #[test]
    fn decodes_postgres_copy_escapes() {
        assert_eq!(
            decode_copy_field(r"line\nnext\tcolumn\\slash").unwrap(),
            Some("line\nnext\tcolumn\\slash".to_string())
        );
        assert_eq!(decode_copy_field(r"\N").unwrap(), None);
    }

    #[test]
    fn builds_text_only_records_from_owned_rows() {
        let dump = concat!(
            "COPY public.\"user\" (id, name, email) FROM stdin;\n",
            "source-user\tname\tkokoybunny@gmail.com\n\\.\n",
            "COPY public.chat (id, user_id, title, old_chat, created_at) FROM stdin;\n",
            "chat-1\tsource-user\t A title \t\\N\t100\n\\.\n",
            "COPY public.chat_message (id, chat_id, user_id, role, parent_id, content, output, model_id, files, sources, embeds, done, status_history, error, usage, created_at) FROM stdin;\n",
            "message-1\tchat-1\tsource-user\tuser\t\\N\t\"hello\\\\nworld\"\t\\N\t\\N\t\\N\t\\N\t\\N\tt\t\\N\t\\N\t\\N\t101\n",
            "message-2\tchat-1\tsource-user\tassistant\t\\N\t\"\"\t\\N\t\\N\t\\N\t\\N\t\\N\tt\t\\N\t\\N\t\\N\t102\n\\.\n"
        );
        let backup = parse_backup(Cursor::new(dump), "kokoybunny@gmail.com").unwrap();
        let (chats, messages) = build_records(backup, "target-user").unwrap();
        assert_eq!(chats.len(), 1);
        assert_eq!(chats[0].title, "A title");
        let messages = messages.get("chat-1").unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].parts[0]["text"], "hello\nworld");
        assert_eq!(messages[1].parts[0]["text"], "");
        assert_eq!(messages[0].attachments, serde_json::json!([]));
    }
}
