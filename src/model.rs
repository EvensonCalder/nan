use serde::{Deserialize, Serialize};

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NativeLanguage {
    English,
    #[default]
    Chinese,
}

impl NativeLanguage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::English => "english",
            Self::Chinese => "chinese",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProficiencyLevel {
    #[default]
    N55,
    N5,
    N45,
    N4,
    N35,
    N3,
    N25,
    N2,
    N15,
    N1,
}

impl ProficiencyLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::N55 => "n5.5",
            Self::N5 => "n5",
            Self::N45 => "n4.5",
            Self::N4 => "n4",
            Self::N35 => "n3.5",
            Self::N3 => "n3",
            Self::N25 => "n2.5",
            Self::N2 => "n2",
            Self::N15 => "n1.5",
            Self::N1 => "n1",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub ref_capacity: usize,
    pub level: ProficiencyLevel,
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    pub romaji_enabled: bool,
    pub furigana_enabled: bool,
    pub lan: NativeLanguage,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ref_capacity: 10,
            level: ProficiencyLevel::default(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: None,
            model: "gpt-4o-mini".to_string(),
            romaji_enabled: true,
            furigana_enabled: true,
            lan: NativeLanguage::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Database {
    pub schema_version: u32,
    pub settings: Settings,
    pub sentences: Vec<SentenceRecord>,
    pub words: Vec<WordRecord>,
    pub language_rewrite: Option<LanguageRewriteState>,
    pub next_sentence_id: u64,
    pub next_word_id: u64,
}

impl Default for Database {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            settings: Settings::default(),
            sentences: Vec::new(),
            words: Vec::new(),
            language_rewrite: None,
            next_sentence_id: 1,
            next_word_id: 1,
        }
    }
}

impl Database {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version == 0 {
            return Err("schema_version must be greater than 0".to_string());
        }

        if self.settings.ref_capacity == 0 {
            return Err("settings.ref_capacity must be greater than 0".to_string());
        }

        Ok(())
    }

    pub fn allocate_sentence_id(&mut self) -> u64 {
        let id = self.next_sentence_id;
        self.next_sentence_id += 1;
        id
    }

    pub fn allocate_word_id(&mut self) -> u64 {
        let id = self.next_word_id;
        self.next_word_id += 1;
        id
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RewriteStatus {
    Pending,
    Done,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RewritePhase {
    Sentences,
    Words,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewriteCursor {
    pub phase: RewritePhase,
    pub index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewriteStats {
    pub total_sentences: usize,
    pub done_sentences: usize,
    pub total_words: usize,
    pub done_words: usize,
    pub failures: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageRewriteState {
    pub from_lan: NativeLanguage,
    pub to_lan: NativeLanguage,
    pub started_at_unix_secs: i64,
    pub updated_at_unix_secs: i64,
    pub cursor: RewriteCursor,
    pub stats: RewriteStats,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SentenceRecord {
    pub id: u64,
    pub lan: NativeLanguage,
    pub source_text: String,
    pub translated_text: String,
    pub style: Option<String>,
    pub created_at_unix_secs: i64,
    pub updated_at_unix_secs: i64,
    pub romaji_line: String,
    pub furigana_line: String,
    pub tokens: Vec<SentenceToken>,
    pub word_ids: Vec<u64>,
    pub rewrite_status: RewriteStatus,
    pub rewrite_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SentenceToken {
    pub surface: String,
    pub reading: Option<String>,
    pub romaji: Option<String>,
    pub lemma: Option<String>,
    pub gloss: Option<String>,
    pub variants: Vec<String>,
    pub spans: Vec<TokenSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenSpan {
    pub text: String,
    pub reading: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WordRecord {
    pub id: u64,
    pub lan: NativeLanguage,
    pub canonical_form: String,
    pub translation: String,
    pub analysis: String,
    pub variants: Vec<String>,
    pub source_sentence_ids: Vec<u64>,
    pub s_last_days: f64,
    pub t_last_unix_secs: i64,
    pub created_at_unix_secs: i64,
    pub updated_at_unix_secs: i64,
    pub rewrite_status: RewriteStatus,
    pub rewrite_error: Option<String>,
}

pub fn normalize_word_key(input: &str) -> String {
    input.trim().to_lowercase()
}
