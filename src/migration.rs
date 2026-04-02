use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tempfile::NamedTempFile;

use crate::error::NanError;
use crate::model::{
    CURRENT_SCHEMA_VERSION, Database, LanguageRewriteState, NativeLanguage, RewriteStatus,
    SentenceRecord, SentenceToken, Settings, TokenSpan, WordRecord, is_japanese_punctuation,
    normalize_word_key,
};

const MIGRATION_STATE_SUFFIX: &str = ".migration-state.json";
const MIGRATION_BACKUP_SUFFIX: &str = ".migration-backup.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct MigrationState {
    from_version: u32,
    target_version: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct DatabaseV1 {
    schema_version: u32,
    settings: Settings,
    sentences: Vec<SentenceRecordV1>,
    words: Vec<WordRecord>,
    language_rewrite: Option<LanguageRewriteState>,
    next_sentence_id: u64,
    next_word_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SentenceRecordV1 {
    id: u64,
    lan: NativeLanguage,
    source_text: String,
    translated_text: String,
    style: Option<String>,
    created_at_unix_secs: i64,
    updated_at_unix_secs: i64,
    romaji_line: String,
    furigana_line: String,
    tokens: Vec<SentenceTokenV1>,
    word_ids: Vec<u64>,
    rewrite_status: RewriteStatus,
    rewrite_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SentenceTokenV1 {
    surface: String,
    reading: Option<String>,
    romaji: Option<String>,
    lemma: Option<String>,
    #[serde(default)]
    gloss: Option<String>,
    variants: Vec<String>,
    spans: Vec<TokenSpan>,
}

pub fn ensure_current_schema(path: &Path) -> Result<(), NanError> {
    let state_path = migration_state_path(path);
    let backup_path = migration_backup_path(path);

    if state_path.exists() {
        recover_interrupted_migration(path, &state_path, &backup_path)?;
    }

    if !path.exists() {
        return Ok(());
    }

    let raw = read_file(path)?;
    let version = detect_schema_version(&raw, path)?;
    if version == CURRENT_SCHEMA_VERSION {
        return Ok(());
    }
    if version > CURRENT_SCHEMA_VERSION {
        return Err(NanError::InvalidData(format!(
            "database schema version {version} is newer than tool support {CURRENT_SCHEMA_VERSION}"
        )));
    }

    let state = MigrationState {
        from_version: version,
        target_version: CURRENT_SCHEMA_VERSION,
    };
    write_json_atomically(&state_path, &state)?;
    write_bytes_atomically(&backup_path, raw.as_bytes())?;

    let database = migrate_raw_to_current(&raw, version, path)?;
    write_database_atomically(path, &database)?;
    cleanup_migration_artifacts(&state_path, &backup_path)?;
    Ok(())
}

fn recover_interrupted_migration(
    path: &Path,
    state_path: &Path,
    backup_path: &Path,
) -> Result<(), NanError> {
    let _state: MigrationState = read_json_file(state_path)?;

    if path.exists() {
        let current_raw = read_file(path)?;
        let current_version = detect_schema_version(&current_raw, path)?;
        if current_version == CURRENT_SCHEMA_VERSION {
            cleanup_migration_artifacts(state_path, backup_path)?;
            return Ok(());
        }
        if current_version > CURRENT_SCHEMA_VERSION {
            return Err(NanError::InvalidData(format!(
                "database schema version {current_version} is newer than tool support {CURRENT_SCHEMA_VERSION}"
            )));
        }
    }

    let source_raw = if backup_path.exists() {
        read_file(backup_path)?
    } else if path.exists() {
        read_file(path)?
    } else {
        return Err(NanError::message(format!(
            "migration recovery could not find either {} or {}",
            path.display(),
            backup_path.display()
        )));
    };

    let source_version = detect_schema_version(&source_raw, path)?;
    if source_version > CURRENT_SCHEMA_VERSION {
        return Err(NanError::InvalidData(format!(
            "database schema version {source_version} is newer than tool support {CURRENT_SCHEMA_VERSION}"
        )));
    }

    let recovery_state = MigrationState {
        from_version: source_version,
        target_version: CURRENT_SCHEMA_VERSION,
    };
    write_json_atomically(state_path, &recovery_state)?;
    if !backup_path.exists() {
        write_bytes_atomically(backup_path, source_raw.as_bytes())?;
    }

    let database = migrate_raw_to_current(&source_raw, source_version, path)?;
    write_database_atomically(path, &database)?;
    cleanup_migration_artifacts(state_path, backup_path)?;
    Ok(())
}

fn migrate_raw_to_current(raw: &str, from_version: u32, path: &Path) -> Result<Database, NanError> {
    let mut value: Value = serde_json::from_str(raw).map_err(|source| NanError::ParseJson {
        path: path.to_path_buf(),
        source,
    })?;
    let mut version = from_version;

    while version < CURRENT_SCHEMA_VERSION {
        value = match version {
            1 => serde_json::to_value(migrate_v1_to_v2(value, path)?).map_err(|source| {
                NanError::SerializeJson {
                    path: path.to_path_buf(),
                    source,
                }
            })?,
            _ => {
                return Err(NanError::InvalidData(format!(
                    "no migration path is implemented from schema version {version}"
                )));
            }
        };
        version += 1;
    }

    let mut database: Database =
        serde_json::from_value(value).map_err(|source| NanError::ParseJson {
            path: path.to_path_buf(),
            source,
        })?;
    database.schema_version = CURRENT_SCHEMA_VERSION;
    database.sanitize();
    database.validate().map_err(NanError::InvalidData)?;
    Ok(database)
}

fn migrate_v1_to_v2(value: Value, path: &Path) -> Result<Database, NanError> {
    let database_v1: DatabaseV1 =
        serde_json::from_value(value).map_err(|source| NanError::ParseJson {
            path: path.to_path_buf(),
            source,
        })?;

    let words = database_v1.words;
    let sentences = database_v1
        .sentences
        .into_iter()
        .map(|sentence| migrate_sentence_v1_to_v2(sentence, &words))
        .collect();

    Ok(Database {
        schema_version: 2,
        settings: database_v1.settings,
        sentences,
        words,
        language_rewrite: database_v1.language_rewrite,
        next_sentence_id: database_v1.next_sentence_id,
        next_word_id: database_v1.next_word_id,
    })
}

fn migrate_sentence_v1_to_v2(sentence: SentenceRecordV1, words: &[WordRecord]) -> SentenceRecord {
    SentenceRecord {
        id: sentence.id,
        lan: sentence.lan,
        source_text: sentence.source_text,
        translated_text: sentence.translated_text,
        style: sentence.style,
        created_at_unix_secs: sentence.created_at_unix_secs,
        updated_at_unix_secs: sentence.updated_at_unix_secs,
        romaji_line: sentence.romaji_line,
        furigana_line: sentence.furigana_line,
        tokens: sentence
            .tokens
            .into_iter()
            .map(|token| migrate_token_v1_to_v2(&token, words))
            .collect(),
        word_ids: sentence.word_ids,
        rewrite_status: sentence.rewrite_status,
        rewrite_error: sentence.rewrite_error,
    }
}

fn migrate_token_v1_to_v2(token: &SentenceTokenV1, words: &[WordRecord]) -> SentenceToken {
    if is_japanese_punctuation(&token.surface) {
        return SentenceToken {
            surface: token.surface.clone(),
            reading: token.reading.clone(),
            romaji: token.romaji.clone(),
            lemma: token.lemma.clone(),
            gloss: None,
            analysis: None,
            context_gloss: None,
            context_analysis: None,
            variants: token.variants.clone(),
            spans: token.spans.clone(),
        };
    }

    let matched_word = find_matching_word_v1(token, words);
    SentenceToken {
        surface: token.surface.clone(),
        reading: token.reading.clone(),
        romaji: token.romaji.clone(),
        lemma: token.lemma.clone(),
        gloss: matched_word
            .map(|word| word.translation.clone())
            .or_else(|| token.gloss.clone()),
        analysis: matched_word.map(|word| word.analysis.clone()),
        context_gloss: token.gloss.clone(),
        context_analysis: None,
        variants: token.variants.clone(),
        spans: token.spans.clone(),
    }
}

fn find_matching_word_v1<'a>(
    token: &SentenceTokenV1,
    words: &'a [WordRecord],
) -> Option<&'a WordRecord> {
    let lookup_values = token_lookup_values_v1(token);
    words.iter().find(|word| {
        let existing_keys = word
            .variants
            .iter()
            .map(|variant| normalize_word_key(variant))
            .chain(std::iter::once(normalize_word_key(&word.canonical_form)))
            .collect::<Vec<_>>();
        lookup_values
            .iter()
            .any(|candidate| existing_keys.contains(candidate))
    })
}

fn token_lookup_values_v1(token: &SentenceTokenV1) -> Vec<String> {
    let mut lookup_values = token
        .variants
        .iter()
        .map(|variant| normalize_word_key(variant))
        .filter(|variant| !variant.is_empty())
        .collect::<Vec<_>>();
    lookup_values.push(normalize_word_key(&token.surface));
    if let Some(lemma) = &token.lemma {
        lookup_values.push(normalize_word_key(lemma));
    }
    lookup_values.sort();
    lookup_values.dedup();
    lookup_values
}

fn detect_schema_version(raw: &str, path: &Path) -> Result<u32, NanError> {
    let value: Value = serde_json::from_str(raw).map_err(|source| NanError::ParseJson {
        path: path.to_path_buf(),
        source,
    })?;
    let version = value
        .get("schema_version")
        .and_then(Value::as_u64)
        .unwrap_or(1);
    u32::try_from(version).map_err(|_| {
        NanError::InvalidData(format!("schema_version {version} does not fit into u32"))
    })
}

fn read_file(path: &Path) -> Result<String, NanError> {
    fs::read_to_string(path).map_err(|source| NanError::ReadFile {
        path: path.to_path_buf(),
        source,
    })
}

fn read_json_file<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, NanError> {
    let raw = read_file(path)?;
    serde_json::from_str(&raw).map_err(|source| NanError::ParseJson {
        path: path.to_path_buf(),
        source,
    })
}

fn write_json_atomically<T: Serialize>(path: &Path, value: &T) -> Result<(), NanError> {
    let serialized =
        serde_json::to_string_pretty(value).map_err(|source| NanError::SerializeJson {
            path: path.to_path_buf(),
            source,
        })?;
    write_bytes_atomically(path, serialized.as_bytes())
}

fn write_database_atomically(path: &Path, database: &Database) -> Result<(), NanError> {
    let serialized =
        serde_json::to_string_pretty(database).map_err(|source| NanError::SerializeJson {
            path: path.to_path_buf(),
            source,
        })?;
    write_bytes_atomically(path, serialized.as_bytes())
}

fn write_bytes_atomically(path: &Path, bytes: &[u8]) -> Result<(), NanError> {
    let parent = path.parent().ok_or_else(|| {
        NanError::InvalidData(format!(
            "path {} does not have a parent directory",
            path.display()
        ))
    })?;

    fs::create_dir_all(parent).map_err(|source| NanError::CreateDirectory {
        path: parent.to_path_buf(),
        source,
    })?;

    let mut temporary_file =
        NamedTempFile::new_in(parent).map_err(|source| NanError::WriteFile {
            path: path.to_path_buf(),
            source,
        })?;
    temporary_file
        .write_all(bytes)
        .map_err(|source| NanError::WriteFile {
            path: path.to_path_buf(),
            source,
        })?;
    temporary_file
        .write_all(b"\n")
        .map_err(|source| NanError::WriteFile {
            path: path.to_path_buf(),
            source,
        })?;
    temporary_file
        .flush()
        .map_err(|source| NanError::WriteFile {
            path: path.to_path_buf(),
            source,
        })?;
    temporary_file
        .persist(path)
        .map_err(|error| NanError::WriteFile {
            path: path.to_path_buf(),
            source: error.error,
        })?;
    Ok(())
}

fn cleanup_migration_artifacts(state_path: &Path, backup_path: &Path) -> Result<(), NanError> {
    remove_if_exists(backup_path)?;
    remove_if_exists(state_path)?;
    Ok(())
}

fn remove_if_exists(path: &Path) -> Result<(), NanError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(NanError::WriteFile {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn migration_state_path(path: &Path) -> PathBuf {
    sidecar_path(path, MIGRATION_STATE_SUFFIX)
}

fn migration_backup_path(path: &Path) -> PathBuf {
    sidecar_path(path, MIGRATION_BACKUP_SUFFIX)
}

fn sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| format!("{name}{suffix}"))
        .unwrap_or_else(|| format!("database{suffix}"));
    path.with_file_name(file_name)
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::{
        DatabaseV1, SentenceRecordV1, SentenceTokenV1, detect_schema_version,
        ensure_current_schema, migration_backup_path, migration_state_path, write_bytes_atomically,
        write_json_atomically,
    };
    use crate::model::{
        CURRENT_SCHEMA_VERSION, Database, NativeLanguage, RewriteStatus, Settings, TokenSpan,
        WordRecord,
    };

    fn sample_v1_database() -> DatabaseV1 {
        DatabaseV1 {
            schema_version: 1,
            settings: Settings::default(),
            sentences: vec![SentenceRecordV1 {
                id: 1,
                lan: NativeLanguage::Chinese,
                source_text: "私はコーヒーを飲みません。".to_string(),
                translated_text: "我不喝咖啡。".to_string(),
                style: None,
                created_at_unix_secs: 0,
                updated_at_unix_secs: 0,
                romaji_line: "watashi wa koohii o nomimasen.".to_string(),
                furigana_line: "私[わたし]は コーヒーを 飲[の]みません。".to_string(),
                tokens: vec![
                    SentenceTokenV1 {
                        surface: "飲みません".to_string(),
                        reading: Some("のみません".to_string()),
                        romaji: Some("nomimasen".to_string()),
                        lemma: Some("飲む".to_string()),
                        gloss: Some("不喝".to_string()),
                        variants: vec!["飲みません".to_string(), "飲む".to_string()],
                        spans: vec![TokenSpan {
                            text: "飲みません".to_string(),
                            reading: Some("のみません".to_string()),
                        }],
                    },
                    SentenceTokenV1 {
                        surface: "。".to_string(),
                        reading: None,
                        romaji: None,
                        lemma: Some("。".to_string()),
                        gloss: Some("句号".to_string()),
                        variants: vec!["。".to_string()],
                        spans: vec![TokenSpan {
                            text: "。".to_string(),
                            reading: None,
                        }],
                    },
                ],
                word_ids: vec![1],
                rewrite_status: RewriteStatus::Done,
                rewrite_error: None,
            }],
            words: vec![WordRecord {
                id: 1,
                lan: NativeLanguage::Chinese,
                canonical_form: "飲む".to_string(),
                translation: "喝".to_string(),
                analysis: "动词原形，表示喝".to_string(),
                variants: vec!["飲む".to_string(), "飲みません".to_string()],
                source_sentence_ids: vec![1],
                s_last_days: 0.018,
                t_last_unix_secs: 0,
                created_at_unix_secs: 0,
                updated_at_unix_secs: 0,
                rewrite_status: RewriteStatus::Done,
                rewrite_error: None,
            }],
            language_rewrite: None,
            next_sentence_id: 2,
            next_word_id: 2,
        }
    }

    #[test]
    fn ensure_current_schema_migrates_v1_to_v2() {
        let temporary_directory = TempDir::new().expect("temp dir should exist");
        let path = temporary_directory.path().join("config.json");
        write_json_atomically(&path, &sample_v1_database()).expect("v1 database should write");

        ensure_current_schema(&path).expect("migration should succeed");

        let raw = std::fs::read_to_string(&path).expect("database should read");
        assert_eq!(
            detect_schema_version(&raw, &path).expect("version should parse"),
            2
        );
        let database: Database = serde_json::from_str(&raw).expect("database should parse");
        assert_eq!(database.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(database.sentences[0].tokens[0].gloss.as_deref(), Some("喝"));
        assert_eq!(
            database.sentences[0].tokens[0].context_gloss.as_deref(),
            Some("不喝")
        );
        assert_eq!(database.sentences[0].tokens[1].gloss, None);
        assert!(!migration_state_path(&path).exists());
        assert!(!migration_backup_path(&path).exists());
    }

    #[test]
    fn ensure_current_schema_recovers_from_interrupted_migration_with_backup() {
        let temporary_directory = TempDir::new().expect("temp dir should exist");
        let path = temporary_directory.path().join("config.json");
        let raw = serde_json::to_string_pretty(&sample_v1_database()).expect("v1 should serialize");
        write_bytes_atomically(&migration_backup_path(&path), raw.as_bytes())
            .expect("backup should write");
        write_json_atomically(
            &migration_state_path(&path),
            &serde_json::json!({"from_version": 1, "target_version": 2}),
        )
        .expect("state should write");

        ensure_current_schema(&path).expect("recovery migration should succeed");

        let raw = std::fs::read_to_string(&path).expect("database should read");
        let database: Database = serde_json::from_str(&raw).expect("database should parse");
        assert_eq!(database.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(database.sentences[0].tokens[0].gloss.as_deref(), Some("喝"));
        assert!(!migration_state_path(&path).exists());
        assert!(!migration_backup_path(&path).exists());
    }

    #[test]
    fn ensure_current_schema_recovers_when_only_state_file_exists() {
        let temporary_directory = TempDir::new().expect("temp dir should exist");
        let path = temporary_directory.path().join("config.json");
        write_json_atomically(&path, &sample_v1_database()).expect("v1 database should write");
        write_json_atomically(
            &migration_state_path(&path),
            &serde_json::json!({"from_version": 1, "target_version": 2}),
        )
        .expect("state should write");

        ensure_current_schema(&path).expect("recovery migration should succeed");

        let raw = std::fs::read_to_string(&path).expect("database should read");
        let database: Database = serde_json::from_str(&raw).expect("database should parse");
        assert_eq!(database.schema_version, CURRENT_SCHEMA_VERSION);
        assert!(!migration_state_path(&path).exists());
        assert!(!migration_backup_path(&path).exists());
    }

    #[test]
    fn ensure_current_schema_cleans_stale_artifacts_after_completed_migration() {
        let temporary_directory = TempDir::new().expect("temp dir should exist");
        let path = temporary_directory.path().join("config.json");
        let database = Database::default();
        write_json_atomically(&path, &database).expect("current database should write");
        write_json_atomically(
            &migration_state_path(&path),
            &serde_json::json!({"from_version": 1, "target_version": 2}),
        )
        .expect("state should write");
        write_bytes_atomically(&migration_backup_path(&path), b"{}").expect("backup should write");

        ensure_current_schema(&path).expect("stale cleanup should succeed");

        assert!(!migration_state_path(&path).exists());
        assert!(!migration_backup_path(&path).exists());
    }

    #[test]
    fn ensure_current_schema_rejects_newer_database_versions() {
        let temporary_directory = TempDir::new().expect("temp dir should exist");
        let path = temporary_directory.path().join("config.json");
        write_bytes_atomically(
            &path,
            br#"{"schema_version":999,"settings":{"ref_capacity":10,"level":"n5.5","base_url":"https://api.openai.com/v1","api_key":null,"model":"gpt-4o-mini","romaji_enabled":true,"furigana_enabled":true,"lan":"chinese"},"sentences":[],"words":[],"language_rewrite":null,"next_sentence_id":1,"next_word_id":1}"#,
        )
        .expect("future database should write");

        let error = ensure_current_schema(&path).expect_err("future version should fail");
        assert!(error.to_string().contains("newer than tool support"));
    }
}
