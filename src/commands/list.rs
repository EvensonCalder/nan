use crate::cli::{ListTarget, resolve_list_args};
use unicode_width::UnicodeWidthStr;

use crate::error::NanError;
use crate::render::render_sentence;
use crate::store::Store;

use super::add::current_unix_secs;
use super::cat::select_sentence_indexes;
use super::new::weakest_word_ids;

pub fn run(store: &Store, first: Option<String>, second: Option<String>) -> Result<(), NanError> {
    let database = store.load_or_create()?;
    let args = resolve_list_args(first.as_deref(), second.as_deref())?;
    let now_unix_secs = current_unix_secs()?;

    match args.target {
        ListTarget::Word => list_words(&database, now_unix_secs, args.count),
        ListTarget::Sentence => list_sentences(&database, now_unix_secs, args.count),
    }
}

fn list_words(
    database: &crate::model::Database,
    now_unix_secs: i64,
    n: Option<isize>,
) -> Result<(), NanError> {
    if database.words.is_empty() {
        return Ok(());
    }

    let mut lines = Vec::new();
    match n {
        Some(value) if value < 0 => {
            let limit = value.unsigned_abs();
            let mut words = database.words.iter().collect::<Vec<_>>();
            words.sort_by(|left, right| right.t_last_unix_secs.cmp(&left.t_last_unix_secs));
            let selected = words.into_iter().take(limit).collect::<Vec<_>>();
            lines.extend(format_word_rows(&selected));
        }
        _ => {
            let mut selected = Vec::new();
            let limit = n
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or(database.words.len());
            for word_id in weakest_word_ids(database, now_unix_secs, limit)? {
                if let Some(word) = database.words.iter().find(|word| word.id == word_id) {
                    selected.push(word);
                }
            }
            lines.extend(format_word_rows(&selected));
        }
    }

    println!("{}", lines.join("\n"));
    Ok(())
}

fn format_word_rows(words: &[&crate::model::WordRecord]) -> Vec<String> {
    if words.is_empty() {
        return Vec::new();
    }

    let word_width = words
        .iter()
        .map(|word| UnicodeWidthStr::width_cjk(word.canonical_form.as_str()))
        .max()
        .unwrap_or(0);
    let translation_width = words
        .iter()
        .map(|word| UnicodeWidthStr::width_cjk(word.translation.as_str()))
        .max()
        .unwrap_or(0);

    words
        .iter()
        .map(|word| {
            format!(
                "{}  {}  {}",
                pad_display_width(&word.canonical_form, word_width),
                pad_display_width(&word.translation, translation_width),
                word.analysis
            )
        })
        .collect()
}

fn pad_display_width(text: &str, width: usize) -> String {
    let current = UnicodeWidthStr::width_cjk(text);
    if current >= width {
        return text.to_string();
    }

    format!("{text}{}", " ".repeat(width - current))
}

fn list_sentences(
    database: &crate::model::Database,
    now_unix_secs: i64,
    n: Option<isize>,
) -> Result<(), NanError> {
    if database.sentences.is_empty() {
        return Ok(());
    }

    let indexes = match n {
        Some(value) if value < 0 => {
            let limit = value.unsigned_abs().min(database.sentences.len());
            let start = database.sentences.len() - limit;
            (start..database.sentences.len()).collect::<Vec<_>>()
        }
        _ => {
            let limit = n
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or(database.sentences.len());
            select_sentence_indexes(database, now_unix_secs, limit)?
        }
    };

    let index_width = indexes
        .iter()
        .map(|index| (index + 1).to_string().len())
        .max()
        .unwrap_or(1);
    let settings = &database.settings;
    let continuation_indent = " ".repeat(index_width + 2);
    let mut lines = Vec::new();
    for index in indexes {
        let sentence = &database.sentences[index];
        let rendered = render_sentence(sentence, settings);
        let mut rendered_lines = rendered.lines();

        if let Some(first_line) = rendered_lines.next() {
            lines.push(format!(
                "{number:>width$}. {line}",
                number = index + 1,
                width = index_width,
                line = first_line,
            ));
        }

        for line in rendered_lines {
            lines.push(format!("{continuation_indent}{line}"));
        }
    }

    println!("{}", lines.join("\n"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::model::{NativeLanguage, RewriteStatus, Settings, WordRecord};

    use super::{format_word_rows, list_sentences, pad_display_width};

    #[test]
    fn pad_display_width_accounts_for_cjk_width() {
        assert_eq!(pad_display_width("私", 4), "私  ");
        assert_eq!(pad_display_width("abc", 5), "abc  ");
    }

    #[test]
    fn format_word_rows_aligns_columns() {
        let words = [
            WordRecord {
                id: 1,
                lan: NativeLanguage::Chinese,
                canonical_form: "私".to_string(),
                translation: "我".to_string(),
                analysis: "第一人称".to_string(),
                variants: vec!["私".to_string()],
                source_sentence_ids: vec![1],
                s_last_days: 0.018,
                t_last_unix_secs: 0,
                created_at_unix_secs: 0,
                updated_at_unix_secs: 0,
                rewrite_status: RewriteStatus::Done,
                rewrite_error: None,
            },
            WordRecord {
                id: 2,
                lan: NativeLanguage::Chinese,
                canonical_form: "コーヒー".to_string(),
                translation: "咖啡".to_string(),
                analysis: "外来语".to_string(),
                variants: vec!["コーヒー".to_string()],
                source_sentence_ids: vec![1],
                s_last_days: 0.018,
                t_last_unix_secs: 0,
                created_at_unix_secs: 0,
                updated_at_unix_secs: 0,
                rewrite_status: RewriteStatus::Done,
                rewrite_error: None,
            },
        ];
        let refs = words.iter().collect::<Vec<_>>();
        let rows = format_word_rows(&refs);
        assert_eq!(rows.len(), 2);
        assert!(rows[0].contains("我    第一人称") || rows[0].contains("我  第一人称"));
    }

    #[test]
    fn sentence_rows_align_translation_indent() {
        let database = crate::model::Database {
            settings: Settings::default(),
            sentences: vec![
                crate::model::SentenceRecord {
                    id: 1,
                    lan: NativeLanguage::Chinese,
                    source_text: "私は学生です。".to_string(),
                    translated_text: "我是学生。".to_string(),
                    style: None,
                    created_at_unix_secs: 0,
                    updated_at_unix_secs: 0,
                    romaji_line: String::new(),
                    furigana_line: String::new(),
                    tokens: Vec::new(),
                    word_ids: Vec::new(),
                    rewrite_status: RewriteStatus::Done,
                    rewrite_error: None,
                },
                crate::model::SentenceRecord {
                    id: 2,
                    lan: NativeLanguage::Chinese,
                    source_text: "今日は雨です。".to_string(),
                    translated_text: "今天下雨。".to_string(),
                    style: None,
                    created_at_unix_secs: 0,
                    updated_at_unix_secs: 0,
                    romaji_line: String::new(),
                    furigana_line: String::new(),
                    tokens: Vec::new(),
                    word_ids: Vec::new(),
                    rewrite_status: RewriteStatus::Done,
                    rewrite_error: None,
                },
            ],
            ..crate::model::Database::default()
        };

        let _ = list_sentences(&database, 0, Some(2));
    }

    #[test]
    fn sentence_rows_use_rendered_sentence_blocks() {
        let database = crate::model::Database {
            settings: Settings::default(),
            sentences: vec![crate::model::SentenceRecord {
                id: 1,
                lan: NativeLanguage::Chinese,
                source_text: "猫です。".to_string(),
                translated_text: "这是猫。".to_string(),
                style: None,
                created_at_unix_secs: 0,
                updated_at_unix_secs: 0,
                romaji_line: "neko desu".to_string(),
                furigana_line: "ねこ です".to_string(),
                tokens: vec![],
                word_ids: vec![],
                rewrite_status: RewriteStatus::Done,
                rewrite_error: None,
            }],
            ..crate::model::Database::default()
        };

        let _ = list_sentences(&database, 0, Some(1));
    }

    #[test]
    fn word_listing_without_count_uses_all_words() {
        let database = crate::model::Database {
            words: vec![
                WordRecord {
                    id: 1,
                    lan: NativeLanguage::Chinese,
                    canonical_form: "私".to_string(),
                    translation: "我".to_string(),
                    analysis: "第一人称".to_string(),
                    variants: vec!["私".to_string()],
                    source_sentence_ids: vec![1],
                    s_last_days: 0.018,
                    t_last_unix_secs: 0,
                    created_at_unix_secs: 0,
                    updated_at_unix_secs: 0,
                    rewrite_status: RewriteStatus::Done,
                    rewrite_error: None,
                },
                WordRecord {
                    id: 2,
                    lan: NativeLanguage::Chinese,
                    canonical_form: "猫".to_string(),
                    translation: "猫".to_string(),
                    analysis: "动物名词".to_string(),
                    variants: vec!["猫".to_string()],
                    source_sentence_ids: vec![1],
                    s_last_days: 0.018,
                    t_last_unix_secs: 0,
                    created_at_unix_secs: 0,
                    updated_at_unix_secs: 0,
                    rewrite_status: RewriteStatus::Done,
                    rewrite_error: None,
                },
            ],
            ..crate::model::Database::default()
        };

        let rows = format_word_rows(&database.words.iter().collect::<Vec<_>>());
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn word_rows_work_without_punctuation_entries() {
        let rows = format_word_rows(&[
            &WordRecord {
                id: 1,
                lan: NativeLanguage::Chinese,
                canonical_form: "匿名さん".to_string(),
                translation: "匿名的人".to_string(),
                analysis: "称呼匿名的人".to_string(),
                variants: vec!["匿名さん".to_string()],
                source_sentence_ids: vec![1],
                s_last_days: 0.018,
                t_last_unix_secs: 0,
                created_at_unix_secs: 0,
                updated_at_unix_secs: 0,
                rewrite_status: RewriteStatus::Done,
                rewrite_error: None,
            },
            &WordRecord {
                id: 2,
                lan: NativeLanguage::Chinese,
                canonical_form: "すごい".to_string(),
                translation: "厉害".to_string(),
                analysis: "常用形容词".to_string(),
                variants: vec!["すごい".to_string()],
                source_sentence_ids: vec![1],
                s_last_days: 0.018,
                t_last_unix_secs: 0,
                created_at_unix_secs: 0,
                updated_at_unix_secs: 0,
                rewrite_status: RewriteStatus::Done,
                rewrite_error: None,
            },
        ]);
        assert_eq!(rows.len(), 2);
    }
}
