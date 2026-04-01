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
            for word in words.into_iter().take(limit) {
                lines.push(format!(
                    "{}: {} | {}",
                    word.canonical_form, word.translation, word.analysis
                ));
            }
        }
        _ => {
            let limit = n
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or(database.words.len());
            for word_id in weakest_word_ids(database, now_unix_secs, limit)? {
                if let Some(word) = database.words.iter().find(|word| word.id == word_id) {
                    lines.push(format!(
                        "{}: {} | {}",
                        word.canonical_form, word.translation, word.analysis
                    ));
                }
            }
        }
    }

    println!("{}", lines.join("\n"));
    Ok(())
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

    let mut lines = Vec::new();
    for index in indexes {
        let sentence = &database.sentences[index];
        lines.push(format!("{}. {}", index + 1, sentence.source_text));
        lines.push(sentence.translated_text.clone());
    }

    println!("{}", lines.join("\n"));
    Ok(())
}
