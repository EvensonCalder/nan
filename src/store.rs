use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use tempfile::NamedTempFile;

use crate::error::NanError;
use crate::model::Database;

pub const CONFIG_FILE_NAME: &str = ".nanconfig.json";

#[derive(Debug, Clone)]
pub struct Store {
    path: PathBuf,
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

    pub fn load_or_create(&self) -> Result<Database, NanError> {
        let mut database = self.load()?;
        if !self.path.exists() {
            self.save(&database)?;
            return Ok(database);
        }

        if database.sanitize() {
            self.save(&database)?;
        }

        Ok(database)
    }

    pub fn save(&self, database: &Database) -> Result<(), NanError> {
        database.validate().map_err(NanError::InvalidData)?;

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
}

pub fn default_config_path() -> Result<PathBuf, NanError> {
    let home_directory = home::home_dir().ok_or(NanError::HomeDirectoryUnavailable)?;
    Ok(home_directory.join(CONFIG_FILE_NAME))
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::Store;
    use crate::model::Database;

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
}
