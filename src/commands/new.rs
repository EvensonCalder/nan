use std::collections::HashSet;

use crate::ai::{AiClient, NewAiResponse};
use crate::cli::resolve_new_args;
use crate::error::NanError;
use crate::model::normalize_word_key;
use crate::prompt::{add_system_prompt, build_new_user_prompt};
use crate::render::render_sentence;
use crate::review::{ReviewState, review_memory_score};
use crate::store::Store;

use super::add::{current_unix_secs, insert_sentence};

pub fn run(store: &Store, first: Option<String>, second: Option<String>) -> Result<(), NanError> {
    let args = resolve_new_args(first.as_deref(), second.as_deref())?;
    let mut database = store.load_or_create()?;
    let settings = database.settings.clone();
    let client = AiClient::from_settings(&settings)?;
    let now_unix_secs = current_unix_secs()?;

    let reference_limit = args.count.saturating_mul(settings.ref_capacity);
    let reference_word_ids = weakest_word_ids(&database, now_unix_secs, reference_limit)?;
    let reference_words = reference_word_ids
        .iter()
        .filter_map(|word_id| database.words.iter().find(|word| word.id == *word_id))
        .map(|word| format!("{} ({})", word.canonical_form, word.translation))
        .collect::<Vec<_>>();
    let reference_sentences = database
        .sentences
        .iter()
        .filter(|sentence| {
            sentence
                .word_ids
                .iter()
                .any(|word_id| reference_word_ids.contains(word_id))
        })
        .map(|sentence| sentence.source_text.clone())
        .take(reference_limit.max(1))
        .collect::<Vec<_>>();

    let prompt = build_new_user_prompt(
        args.count,
        args.style.as_deref(),
        settings.level,
        settings.lan,
        &reference_words,
        &reference_sentences,
    );
    let response: NewAiResponse = client.chat_json(add_system_prompt(), &prompt)?;

    let mut seen_sentences = database
        .sentences
        .iter()
        .map(|sentence| normalize_word_key(&sentence.source_text))
        .collect::<HashSet<_>>();
    let mut rendered = Vec::new();

    for candidate in response.sentences {
        let normalized = normalize_word_key(&candidate.japanese_sentence);
        if normalized.is_empty() || !seen_sentences.insert(normalized) {
            continue;
        }

        let sentence_record =
            insert_sentence(&mut database, candidate, args.style.clone(), now_unix_secs);
        rendered.push(render_sentence(&sentence_record, &settings));
        if rendered.len() >= args.count {
            break;
        }
    }

    if rendered.is_empty() {
        return Err(NanError::message(
            "AI did not produce any new unique sentences to add",
        ));
    }

    store.save(&database)?;
    println!("{}", rendered.join("\n\n"));
    Ok(())
}

pub(crate) fn weakest_word_ids(
    database: &crate::model::Database,
    now_unix_secs: i64,
    limit: usize,
) -> Result<Vec<u64>, NanError> {
    let mut scored = database
        .words
        .iter()
        .map(|word| {
            let state = ReviewState {
                s_last_days: word.s_last_days,
                t_last_unix_secs: word.t_last_unix_secs,
            };
            let score = review_memory_score(state, now_unix_secs)?;
            Ok((word.id, score))
        })
        .collect::<Result<Vec<_>, NanError>>()?;
    scored.sort_by(|left, right| left.1.total_cmp(&right.1));
    if limit == 0 {
        return Ok(Vec::new());
    }

    Ok(scored
        .into_iter()
        .take(limit)
        .map(|(word_id, _)| word_id)
        .collect())
}

#[cfg(test)]
mod tests {
    use crate::model::Database;

    use super::weakest_word_ids;

    #[test]
    fn weakest_words_are_sorted_by_memory_score() {
        let mut database = Database::default();
        database.words.push(crate::model::WordRecord {
            id: 1,
            lan: crate::model::NativeLanguage::Chinese,
            canonical_form: "食べる".to_string(),
            translation: "eat".to_string(),
            analysis: "to eat".to_string(),
            variants: vec!["食べる".to_string()],
            source_sentence_ids: vec![1],
            s_last_days: 1.0,
            t_last_unix_secs: 0,
            created_at_unix_secs: 0,
            updated_at_unix_secs: 0,
            rewrite_status: crate::model::RewriteStatus::Done,
            rewrite_error: None,
        });
        database.words.push(crate::model::WordRecord {
            id: 2,
            lan: crate::model::NativeLanguage::Chinese,
            canonical_form: "見る".to_string(),
            translation: "see".to_string(),
            analysis: "to see".to_string(),
            variants: vec!["見る".to_string()],
            source_sentence_ids: vec![1],
            s_last_days: 10.0,
            t_last_unix_secs: 0,
            created_at_unix_secs: 0,
            updated_at_unix_secs: 0,
            rewrite_status: crate::model::RewriteStatus::Done,
            rewrite_error: None,
        });

        let weakest = weakest_word_ids(&database, 86_400 * 2, 2).expect("scores should calculate");
        assert_eq!(weakest[0], 1);
    }
}
