use serde::{Deserialize, Serialize};

pub const CURRENT_SCHEMA_VERSION: u32 = 2;

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
pub enum ProficiencyLevel {
    #[default]
    #[serde(rename = "n5.5", alias = "n55")]
    N55,
    #[serde(rename = "n5")]
    N5,
    #[serde(rename = "n4.5", alias = "n45")]
    N45,
    #[serde(rename = "n4")]
    N4,
    #[serde(rename = "n3.5", alias = "n35")]
    N35,
    #[serde(rename = "n3")]
    N3,
    #[serde(rename = "n2.5", alias = "n25")]
    N25,
    #[serde(rename = "n2")]
    N2,
    #[serde(rename = "n1.5", alias = "n15")]
    N15,
    #[serde(rename = "n1")]
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

        if self.schema_version > CURRENT_SCHEMA_VERSION {
            return Err(format!(
                "schema_version {} is newer than supported {}",
                self.schema_version, CURRENT_SCHEMA_VERSION
            ));
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

    pub fn sanitize(&mut self) -> bool {
        let punctuation_word_ids = self
            .words
            .iter()
            .filter(|word| is_japanese_punctuation(&word.canonical_form))
            .map(|word| word.id)
            .collect::<std::collections::HashSet<_>>();

        if punctuation_word_ids.is_empty() {
            return false;
        }

        self.words
            .retain(|word| !punctuation_word_ids.contains(&word.id));
        for sentence in &mut self.sentences {
            sentence
                .word_ids
                .retain(|word_id| !punctuation_word_ids.contains(word_id));
        }

        let next_word_id = self.words.iter().map(|word| word.id).max().unwrap_or(0) + 1;
        self.next_word_id = self.next_word_id.max(next_word_id);
        true
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
    #[serde(default)]
    pub gloss: Option<String>,
    #[serde(default)]
    pub analysis: Option<String>,
    #[serde(default)]
    pub context_gloss: Option<String>,
    #[serde(default)]
    pub context_analysis: Option<String>,
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

pub fn is_japanese_punctuation(text: &str) -> bool {
    !text.trim().is_empty()
        && text.chars().all(|character| {
            matches!(
                character,
                '。' | '、'
                    | '「'
                    | '」'
                    | '『'
                    | '』'
                    | '（'
                    | '）'
                    | '［'
                    | '］'
                    | '【'
                    | '】'
                    | '〈'
                    | '〉'
                    | '《'
                    | '》'
                    | '〔'
                    | '〕'
                    | '！'
                    | '？'
                    | 'ー'
                    | '…'
                    | '・'
                    | '〜'
                    | '：'
                    | '；'
                    | '，'
                    | '．'
                    | ','
                    | '.'
                    | '!'
                    | '?'
                    | ':'
                    | ';'
            )
        })
}

#[cfg(test)]
mod tests {
    use super::{Database, ProficiencyLevel, WordRecord, is_japanese_punctuation};

    #[test]
    fn proficiency_level_serializes_with_documented_values() {
        let serialized =
            serde_json::to_string(&ProficiencyLevel::N55).expect("level should serialize");
        assert_eq!(serialized, "\"n5.5\"");
    }

    #[test]
    fn proficiency_level_accepts_legacy_compact_values() {
        let parsed: ProficiencyLevel =
            serde_json::from_str("\"n55\"").expect("legacy level should parse");
        assert_eq!(parsed, ProficiencyLevel::N55);
    }

    #[test]
    fn punctuation_detection_covers_japanese_marks() {
        assert!(is_japanese_punctuation("。"));
        assert!(is_japanese_punctuation("、"));
        assert!(is_japanese_punctuation("？！"));
        assert!(!is_japanese_punctuation("匿名"));
    }

    #[test]
    fn database_sanitize_removes_punctuation_words() {
        let mut database = Database {
            words: vec![
                WordRecord {
                    id: 1,
                    lan: super::NativeLanguage::Chinese,
                    canonical_form: "。".to_string(),
                    translation: "句号".to_string(),
                    analysis: "标点".to_string(),
                    variants: vec!["。".to_string()],
                    source_sentence_ids: vec![1],
                    s_last_days: 0.018,
                    t_last_unix_secs: 0,
                    created_at_unix_secs: 0,
                    updated_at_unix_secs: 0,
                    rewrite_status: super::RewriteStatus::Done,
                    rewrite_error: None,
                },
                WordRecord {
                    id: 2,
                    lan: super::NativeLanguage::Chinese,
                    canonical_form: "猫".to_string(),
                    translation: "猫".to_string(),
                    analysis: "名词".to_string(),
                    variants: vec!["猫".to_string()],
                    source_sentence_ids: vec![1],
                    s_last_days: 0.018,
                    t_last_unix_secs: 0,
                    created_at_unix_secs: 0,
                    updated_at_unix_secs: 0,
                    rewrite_status: super::RewriteStatus::Done,
                    rewrite_error: None,
                },
            ],
            sentences: vec![super::SentenceRecord {
                id: 1,
                lan: super::NativeLanguage::Chinese,
                source_text: "猫。".to_string(),
                translated_text: "猫。".to_string(),
                style: None,
                created_at_unix_secs: 0,
                updated_at_unix_secs: 0,
                romaji_line: String::new(),
                furigana_line: String::new(),
                tokens: Vec::new(),
                word_ids: vec![1, 2],
                rewrite_status: super::RewriteStatus::Done,
                rewrite_error: None,
            }],
            ..Database::default()
        };

        assert!(database.sanitize());
        assert_eq!(database.words.len(), 1);
        assert_eq!(database.sentences[0].word_ids, vec![2]);
    }
}
