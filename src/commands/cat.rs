use std::collections::HashSet;

use crate::error::NanError;
use crate::render::render_sentence;
use crate::review::{ReviewState, apply_review, review_memory_score, review_priority};
use crate::store::Store;

use super::add::current_unix_secs;

pub fn run(store: &Store, n: Option<usize>) -> Result<(), NanError> {
    let mut database = store.load_or_create()?;
    if database.sentences.is_empty() {
        return Err(NanError::message("there are no sentences to review yet"));
    }

    let now_unix_secs = current_unix_secs()?;
    let count = n.unwrap_or(1).max(1);
    let selected_indexes = select_sentence_indexes(&database, now_unix_secs, count)?;
    if selected_indexes.is_empty() {
        return Err(NanError::message("there are no sentences to review yet"));
    }

    let settings = database.settings.clone();
    let mut rendered = Vec::new();
    let mut reviewed_word_ids = HashSet::new();

    for index in &selected_indexes {
        let sentence = &database.sentences[*index];
        rendered.push(render_sentence(sentence, &settings));
        for word_id in &sentence.word_ids {
            reviewed_word_ids.insert(*word_id);
        }
    }

    for word in &mut database.words {
        if reviewed_word_ids.contains(&word.id) {
            let state = ReviewState {
                s_last_days: word.s_last_days,
                t_last_unix_secs: word.t_last_unix_secs,
            };
            let updated = apply_review(state, now_unix_secs)?;
            word.s_last_days = updated.s_last_days;
            word.t_last_unix_secs = updated.t_last_unix_secs;
            word.updated_at_unix_secs = now_unix_secs;
        }
    }

    store.save(&database)?;
    println!("{}", rendered.join("\n\n"));
    Ok(())
}

pub(crate) fn select_sentence_indexes(
    database: &crate::model::Database,
    now_unix_secs: i64,
    count: usize,
) -> Result<Vec<usize>, NanError> {
    let mut word_weights = std::collections::HashMap::new();
    for word in &database.words {
        let score = review_memory_score(
            ReviewState {
                s_last_days: word.s_last_days,
                t_last_unix_secs: word.t_last_unix_secs,
            },
            now_unix_secs,
        )?;
        word_weights.insert(word.id, review_priority(score));
    }

    let mut selected = Vec::new();
    let mut covered_word_ids = HashSet::new();
    let target_count = count.min(database.sentences.len());

    while selected.len() < target_count {
        let mut best_index = None;
        let mut best_gain = -1.0_f64;
        let mut best_total = -1.0_f64;

        for (index, sentence) in database.sentences.iter().enumerate() {
            if selected.contains(&index) {
                continue;
            }

            let total_weight = sentence
                .word_ids
                .iter()
                .map(|word_id| *word_weights.get(word_id).unwrap_or(&0.0))
                .sum::<f64>();
            let uncovered_gain = sentence
                .word_ids
                .iter()
                .filter(|word_id| !covered_word_ids.contains(*word_id))
                .map(|word_id| *word_weights.get(word_id).unwrap_or(&0.0))
                .sum::<f64>();

            if uncovered_gain > best_gain
                || (uncovered_gain == best_gain && total_weight > best_total)
            {
                best_index = Some(index);
                best_gain = uncovered_gain;
                best_total = total_weight;
            }
        }

        let Some(best_index) = best_index else {
            break;
        };

        for word_id in &database.sentences[best_index].word_ids {
            covered_word_ids.insert(*word_id);
        }
        selected.push(best_index);
    }

    Ok(selected)
}

#[cfg(test)]
mod tests {
    use crate::model::{
        Database, NativeLanguage, RewriteStatus, SentenceRecord, Settings, WordRecord,
    };

    use super::select_sentence_indexes;

    #[test]
    fn selection_prefers_covering_more_weak_words() {
        let mut database = Database {
            settings: Settings::default(),
            ..Database::default()
        };
        database.words = vec![
            WordRecord {
                id: 1,
                lan: NativeLanguage::Chinese,
                canonical_form: "食べる".to_string(),
                translation: "eat".to_string(),
                analysis: "eat".to_string(),
                variants: vec!["食べる".to_string()],
                source_sentence_ids: vec![1, 2],
                s_last_days: 1.0,
                t_last_unix_secs: 0,
                created_at_unix_secs: 0,
                updated_at_unix_secs: 0,
                rewrite_status: RewriteStatus::Done,
                rewrite_error: None,
            },
            WordRecord {
                id: 2,
                lan: NativeLanguage::Chinese,
                canonical_form: "見る".to_string(),
                translation: "see".to_string(),
                analysis: "see".to_string(),
                variants: vec!["見る".to_string()],
                source_sentence_ids: vec![1],
                s_last_days: 1.0,
                t_last_unix_secs: 0,
                created_at_unix_secs: 0,
                updated_at_unix_secs: 0,
                rewrite_status: RewriteStatus::Done,
                rewrite_error: None,
            },
        ];
        database.sentences = vec![
            SentenceRecord {
                id: 1,
                lan: NativeLanguage::Chinese,
                source_text: "映画を見てパンを食べます。".to_string(),
                translated_text: "I watch a movie and eat bread.".to_string(),
                style: None,
                created_at_unix_secs: 0,
                updated_at_unix_secs: 0,
                romaji_line: String::new(),
                furigana_line: String::new(),
                tokens: Vec::new(),
                word_ids: vec![1, 2],
                rewrite_status: RewriteStatus::Done,
                rewrite_error: None,
            },
            SentenceRecord {
                id: 2,
                lan: NativeLanguage::Chinese,
                source_text: "パンを食べます。".to_string(),
                translated_text: "I eat bread.".to_string(),
                style: None,
                created_at_unix_secs: 0,
                updated_at_unix_secs: 0,
                romaji_line: String::new(),
                furigana_line: String::new(),
                tokens: Vec::new(),
                word_ids: vec![1],
                rewrite_status: RewriteStatus::Done,
                rewrite_error: None,
            },
        ];

        let selected =
            select_sentence_indexes(&database, 86_400 * 2, 1).expect("selection should work");
        assert_eq!(selected, vec![0]);
    }
}
