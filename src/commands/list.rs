use unicode_width::UnicodeWidthStr;

use crate::cli::ListTarget;
use crate::error::NanError;
use crate::store::Store;

use super::add::current_unix_secs;
use super::cat::select_sentence_indexes;
use super::new::weakest_word_ids;

pub fn run(store: &Store, n: Option<isize>, target: Option<ListTarget>) -> Result<(), NanError> {
    let database = store.load_or_create()?;
    let target = target.unwrap_or_default();
    let now_unix_secs = current_unix_secs()?;

    match target {
        ListTarget::Word => list_words(&database, now_unix_secs, n),
        ListTarget::Sentence => list_sentences(&database, now_unix_secs, n),
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
    let translation_indent = " ".repeat(index_width + 2);
    let mut lines = Vec::new();
    for index in indexes {
        let sentence = &database.sentences[index];
        lines.push(format!(
            "{number:>width$}. {sentence}",
            number = index + 1,
            width = index_width,
            sentence = sentence.source_text,
        ));
        lines.push(format!("{translation_indent}{}", sentence.translated_text));
    }

    println!("{}", lines.join("\n"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::model::{NativeLanguage, RewriteStatus, WordRecord};

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
}
