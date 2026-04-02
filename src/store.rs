use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use tempfile::NamedTempFile;

use crate::error::NanError;
use crate::migration;
use crate::model::Database;

pub const CONFIG_FILE_NAME: &str = ".nanconfig.json";
const LOCK_RETRY_DELAY: Duration = Duration::from_millis(25);
const LOCK_RETRY_ATTEMPTS: usize = 200;

#[derive(Debug, Clone)]
pub struct Store {
    path: PathBuf,
}

#[derive(Debug)]
pub struct StoreLock {
    path: PathBuf,
}

impl Drop for StoreLock {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_file(&self.path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            eprintln!(
                "warning: failed to remove config lock {}: {error}",
                self.path.display()
            );
        }
    }
}

impl Store {
    pub fn new() -> Result<Self, NanError> {
        Ok(Self {
            path: default_config_path()?,
        })
    }

    pub fn with_path(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<Database, NanError> {
        let _lock = self.lock()?;
        self.load_unlocked()
    }

    pub fn load_or_create(&self) -> Result<Database, NanError> {
        let _lock = self.lock()?;
        self.load_or_create_unlocked()
    }

    pub fn save(&self, database: &Database) -> Result<(), NanError> {
        let _lock = self.lock()?;
        self.save_unlocked(database)
    }

    pub fn lock(&self) -> Result<StoreLock, NanError> {
        let parent = self.path.parent().ok_or_else(|| {
            NanError::InvalidData(format!(
                "config path {} does not have a parent directory",
                self.path.display()
            ))
        })?;

        fs::create_dir_all(parent).map_err(|source| NanError::CreateDirectory {
            path: parent.to_path_buf(),
            source,
        })?;

        let lock_path = self.lock_path();
        for _ in 0..LOCK_RETRY_ATTEMPTS {
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock_path)
            {
                Ok(mut file) => {
                    let _ = writeln!(file, "pid={}", std::process::id());
                    return Ok(StoreLock { path: lock_path });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    thread::sleep(LOCK_RETRY_DELAY);
                }
                Err(source) => {
                    return Err(NanError::WriteFile {
                        path: lock_path,
                        source,
                    });
                }
            }
        }

        Err(NanError::message(format!(
            "timed out waiting for config lock at {}",
            lock_path.display()
        )))
    }

    pub(crate) fn load_unlocked(&self) -> Result<Database, NanError> {
        migration::ensure_current_schema(&self.path)?;

        if !self.path.exists() {
            return Ok(Database::default());
        }

        let contents = fs::read_to_string(&self.path).map_err(|source| NanError::ReadFile {
            path: self.path.clone(),
            source,
        })?;
        let mut database: Database =
            serde_json::from_str(&contents).map_err(|source| NanError::ParseJson {
                path: self.path.clone(),
                source,
            })?;

        database.sanitize();
        database.validate().map_err(NanError::InvalidData)?;
        Ok(database)
    }

    pub(crate) fn load_or_create_unlocked(&self) -> Result<Database, NanError> {
        let mut database = self.load_unlocked()?;
        if !self.path.exists() {
            self.save_unlocked(&database)?;
            return Ok(database);
        }

        if database.sanitize() {
            self.save_unlocked(&database)?;
        }

        Ok(database)
    }

    pub(crate) fn save_unlocked(&self, database: &Database) -> Result<(), NanError> {
        database.validate().map_err(NanError::InvalidData)?;
        if database.schema_version != crate::model::CURRENT_SCHEMA_VERSION {
            return Err(NanError::InvalidData(format!(
                "cannot save schema version {}; current tool schema is {}",
                database.schema_version,
                crate::model::CURRENT_SCHEMA_VERSION
            )));
        }

        let parent = self.path.parent().ok_or_else(|| {
            NanError::InvalidData(format!(
                "config path {} does not have a parent directory",
                self.path.display()
            ))
        })?;

        fs::create_dir_all(parent).map_err(|source| NanError::CreateDirectory {
            path: parent.to_path_buf(),
            source,
        })?;

        let serialized =
            serde_json::to_string_pretty(database).map_err(|source| NanError::SerializeJson {
                path: self.path.clone(),
                source,
            })?;

        let mut temporary_file =
            NamedTempFile::new_in(parent).map_err(|source| NanError::WriteFile {
                path: parent.to_path_buf(),
                source,
            })?;

        temporary_file
            .write_all(serialized.as_bytes())
            .map_err(|source| NanError::WriteFile {
                path: self.path.clone(),
                source,
            })?;
        temporary_file
            .write_all(b"\n")
            .map_err(|source| NanError::WriteFile {
                path: self.path.clone(),
                source,
            })?;
        temporary_file
            .flush()
            .map_err(|source| NanError::WriteFile {
                path: self.path.clone(),
                source,
            })?;

        temporary_file
            .persist(&self.path)
            .map_err(|error| NanError::WriteFile {
                path: self.path.clone(),
                source: error.error,
            })?;

        Ok(())
    }

    fn lock_path(&self) -> PathBuf {
        let file_name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| format!("{name}.lock"))
            .unwrap_or_else(|| format!("{CONFIG_FILE_NAME}.lock"));
        self.path.with_file_name(file_name)
    }
}

pub fn default_config_path() -> Result<PathBuf, NanError> {
    let home_directory = home::home_dir().ok_or(NanError::HomeDirectoryUnavailable)?;
    Ok(home_directory.join(CONFIG_FILE_NAME))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant};

    use serde_json::json;
    use tempfile::TempDir;

    use super::Store;
    use crate::model::{Database, ProficiencyLevel};

    #[test]
    fn load_missing_file_returns_default_database() {
        let temporary_directory = TempDir::new().expect("temp dir should exist");
        let path = temporary_directory.path().join("config.json");
        let store = Store::with_path(path);

        let database = store.load().expect("store should load default database");
        assert_eq!(database, Database::default());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let temporary_directory = TempDir::new().expect("temp dir should exist");
        let path = temporary_directory.path().join("config.json");
        let store = Store::with_path(path.clone());
        let mut database = Database::default();
        database.settings.model = "test-model".to_string();

        store.save(&database).expect("store should save database");
        let loaded = store.load().expect("store should reload database");
        assert_eq!(loaded, database);
        assert!(path.exists());
    }

    #[test]
    fn load_or_create_rewrites_sanitized_database() {
        let temporary_directory = TempDir::new().expect("temp dir should exist");
        let path = temporary_directory.path().join("config.json");
        let store = Store::with_path(path.clone());
        let mut database = Database::default();
        database.words.push(crate::model::WordRecord {
            id: 1,
            lan: crate::model::NativeLanguage::Chinese,
            canonical_form: "。".to_string(),
            translation: "句号".to_string(),
            analysis: "标点".to_string(),
            variants: vec!["。".to_string()],
            source_sentence_ids: vec![1],
            s_last_days: 0.018,
            t_last_unix_secs: 0,
            created_at_unix_secs: 0,
            updated_at_unix_secs: 0,
            rewrite_status: crate::model::RewriteStatus::Done,
            rewrite_error: None,
        });

        store.save(&database).expect("store should save database");
        let loaded = store
            .load_or_create()
            .expect("store should sanitize database");
        assert!(loaded.words.is_empty());
    }

    #[test]
    fn lock_serializes_read_modify_write_transactions() {
        let temporary_directory = TempDir::new().expect("temp dir should exist");
        let path = temporary_directory.path().join("config.json");
        let store = Store::with_path(path.clone());
        store
            .save(&Database::default())
            .expect("store should initialize database");

        let guard = store.lock().expect("first lock should succeed");
        let mut first = store
            .load_or_create_unlocked()
            .expect("first transaction should load");
        first.settings.ref_capacity = 7;

        let (sender, receiver) = mpsc::channel();
        let thread_path = path.clone();
        let handle = thread::spawn(move || {
            let store = Store::with_path(thread_path);
            sender.send(()).expect("thread should signal start");
            let started = Instant::now();
            let _guard = store.lock().expect("second lock should succeed");
            let mut database = store
                .load_or_create_unlocked()
                .expect("second transaction should load");
            database.settings.level = ProficiencyLevel::N2;
            store
                .save_unlocked(&database)
                .expect("second transaction should save");
            started.elapsed()
        });

        receiver.recv().expect("thread should start");
        thread::sleep(Duration::from_millis(100));
        store
            .save_unlocked(&first)
            .expect("first transaction should save");
        drop(guard);

        let wait_duration = handle.join().expect("thread should finish");
        assert!(wait_duration >= Duration::from_millis(100));

        let loaded = store.load().expect("store should load merged state");
        assert_eq!(loaded.settings.ref_capacity, 7);
        assert_eq!(loaded.settings.level, ProficiencyLevel::N2);
    }

    #[test]
    fn load_auto_migrates_legacy_schema_before_parsing() {
        let temporary_directory = TempDir::new().expect("temp dir should exist");
        let path = temporary_directory.path().join("config.json");
        fs::write(
            &path,
            json!({
                "schema_version": 1,
                "settings": {
                    "ref_capacity": 10,
                    "level": "n5.5",
                    "base_url": "https://api.openai.com/v1",
                    "api_key": null,
                    "model": "gpt-4o-mini",
                    "romaji_enabled": true,
                    "furigana_enabled": true,
                    "lan": "chinese"
                },
                "sentences": [
                    {
                        "id": 1,
                        "lan": "chinese",
                        "source_text": "私はコーヒーを飲みません。",
                        "translated_text": "我不喝咖啡。",
                        "style": null,
                        "created_at_unix_secs": 0,
                        "updated_at_unix_secs": 0,
                        "romaji_line": "watashi wa koohii o nomimasen.",
                        "furigana_line": "私[わたし]は コーヒーを 飲[の]みません。",
                        "tokens": [
                            {
                                "surface": "飲みません",
                                "reading": "のみません",
                                "romaji": "nomimasen",
                                "lemma": "飲む",
                                "gloss": "不喝",
                                "variants": ["飲みません", "飲む"],
                                "spans": [{"text": "飲みません", "reading": "のみません"}]
                            }
                        ],
                        "word_ids": [1],
                        "rewrite_status": "done",
                        "rewrite_error": null
                    }
                ],
                "words": [
                    {
                        "id": 1,
                        "lan": "chinese",
                        "canonical_form": "飲む",
                        "translation": "喝",
                        "analysis": "动词原形，表示喝",
                        "variants": ["飲みません", "飲む"],
                        "source_sentence_ids": [1],
                        "s_last_days": 0.018,
                        "t_last_unix_secs": 0,
                        "created_at_unix_secs": 0,
                        "updated_at_unix_secs": 0,
                        "rewrite_status": "done",
                        "rewrite_error": null
                    }
                ],
                "language_rewrite": null,
                "next_sentence_id": 2,
                "next_word_id": 2
            })
            .to_string(),
        )
        .expect("legacy config should write");

        let store = Store::with_path(path.clone());
        let database = store
            .load()
            .expect("store should auto-migrate legacy database");
        assert_eq!(
            database.schema_version,
            crate::model::CURRENT_SCHEMA_VERSION
        );
        assert_eq!(database.sentences[0].tokens[0].gloss.as_deref(), Some("喝"));
        assert_eq!(
            database.sentences[0].tokens[0].context_gloss.as_deref(),
            Some("不喝")
        );
    }
}
