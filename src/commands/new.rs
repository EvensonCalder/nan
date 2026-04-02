use std::collections::HashSet;

use crate::ai::{AddAiResponse, AiClient, NewAiResponse};
use crate::cli::resolve_new_args;
use crate::error::NanError;
use crate::model::{SentenceRecord, normalize_word_key};
use crate::prompt::{add_system_prompt, build_new_user_prompt};
use crate::render::render_sentence;
use crate::review::{ReviewState, review_memory_score};
use crate::store::Store;

use super::add::{current_unix_secs, insert_sentence};

const HIGH_SIMILARITY_THRESHOLD: f64 = 0.8;
const MIN_SHARED_CONTENT_WORDS: usize = 3;

pub fn run(store: &Store, first: Option<String>, second: Option<String>) -> Result<(), NanError> {
    let args = resolve_new_args(first.as_deref(), second.as_deref())?;
    let _lock = store.lock()?;
    let mut database = store.load_or_create_unlocked()?;
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
    let mut accepted_signatures = database
        .sentences
        .iter()
        .map(sentence_similarity_signature)
        .filter(|signature| !signature.is_empty())
        .collect::<Vec<_>>();
    let mut rendered = Vec::new();

    for candidate in response.sentences {
        let normalized = normalize_word_key(&candidate.japanese_sentence);
        if normalized.is_empty() || !seen_sentences.insert(normalized) {
            continue;
        }
        let candidate_signature = candidate_similarity_signature(&candidate);
        if is_highly_similar_to_any(&candidate_signature, &accepted_signatures) {
            continue;
        }

        let sentence_record =
            insert_sentence(&mut database, candidate, args.style.clone(), now_unix_secs);
        accepted_signatures.push(sentence_similarity_signature(&sentence_record));
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

    store.save_unlocked(&database)?;
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

fn sentence_similarity_signature(sentence: &SentenceRecord) -> HashSet<String> {
    let mut keys = sentence
        .tokens
        .iter()
        .filter_map(|token| token_similarity_key(token.lemma.as_deref(), &token.surface))
        .filter(|key| !is_function_word(key))
        .collect::<HashSet<_>>();

    if keys.is_empty() {
        keys = sentence
            .tokens
            .iter()
            .filter_map(|token| token_similarity_key(token.lemma.as_deref(), &token.surface))
            .collect::<HashSet<_>>();
    }

    keys
}

fn candidate_similarity_signature(candidate: &AddAiResponse) -> HashSet<String> {
    let mut keys = candidate
        .tokens
        .iter()
        .filter_map(|token| token_similarity_key(token.lemma.as_deref(), &token.surface))
        .filter(|key| !is_function_word(key))
        .collect::<HashSet<_>>();

    if keys.is_empty() {
        keys = candidate
            .tokens
            .iter()
            .filter_map(|token| token_similarity_key(token.lemma.as_deref(), &token.surface))
            .collect::<HashSet<_>>();
    }

    keys
}

fn token_similarity_key(lemma: Option<&str>, surface: &str) -> Option<String> {
    let key = normalize_word_key(lemma.unwrap_or(surface));
    if key.is_empty() || looks_like_punctuation(&key) {
        None
    } else {
        Some(key)
    }
}

fn is_highly_similar_to_any(candidate: &HashSet<String>, existing: &[HashSet<String>]) -> bool {
    if candidate.is_empty() {
        return false;
    }

    existing
        .iter()
        .any(|signature| is_highly_similar(candidate, signature))
}

fn is_highly_similar(left: &HashSet<String>, right: &HashSet<String>) -> bool {
    if left.is_empty() || right.is_empty() {
        return false;
    }

    let shared = left.intersection(right).count();
    if shared < MIN_SHARED_CONTENT_WORDS {
        return false;
    }

    let denominator = left.len().min(right.len());
    if denominator == 0 {
        return false;
    }

    (shared as f64 / denominator as f64) >= HIGH_SIMILARITY_THRESHOLD
}

fn looks_like_punctuation(text: &str) -> bool {
    text.chars().all(|character| {
        matches!(
            character,
            '。' | '、'
                | '！'
                | '？'
                | '.'
                | ','
                | '!'
                | '?'
                | ':'
                | ';'
                | '，'
                | '：'
                | '；'
                | '「'
                | '」'
                | '『'
                | '』'
        )
    })
}

fn is_function_word(key: &str) -> bool {
    matches!(
        key,
        "は" | "が"
            | "を"
            | "に"
            | "へ"
            | "で"
            | "と"
            | "も"
            | "の"
            | "ね"
            | "よ"
            | "か"
            | "な"
            | "や"
            | "から"
            | "まで"
            | "です"
            | "だ"
            | "ます"
    )
}

#[cfg(test)]
mod tests {
    use crate::ai::{AddAiResponse, AddAiSpan, AddAiToken};
    use crate::model::{
        Database, NativeLanguage, RewriteStatus, SentenceRecord, SentenceToken, TokenSpan,
    };

    use super::{
        candidate_similarity_signature, is_highly_similar, sentence_similarity_signature,
        weakest_word_ids,
    };

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

    #[test]
    fn similarity_filter_rejects_near_rephrasings_by_word_overlap() {
        let stored = SentenceRecord {
            id: 1,
            lan: NativeLanguage::Chinese,
            source_text: "私は今日コーヒーを飲みます。".to_string(),
            translated_text: "我今天喝咖啡。".to_string(),
            style: None,
            created_at_unix_secs: 0,
            updated_at_unix_secs: 0,
            romaji_line: String::new(),
            furigana_line: String::new(),
            tokens: vec![
                token("私", Some("私")),
                token("は", Some("は")),
                token("今日", Some("今日")),
                token("コーヒー", Some("コーヒー")),
                token("を", Some("を")),
                token("飲みます", Some("飲む")),
            ],
            word_ids: vec![1],
            rewrite_status: RewriteStatus::Done,
            rewrite_error: None,
        };
        let candidate = AddAiResponse {
            japanese_sentence: "今日は私がコーヒーを飲みます。".to_string(),
            translated_sentence: "今天我来喝咖啡。".to_string(),
            romaji_line: String::new(),
            furigana_line: String::new(),
            tokens: vec![
                ai_token("今日", Some("今日")),
                ai_token("は", Some("は")),
                ai_token("私", Some("私")),
                ai_token("が", Some("が")),
                ai_token("コーヒー", Some("コーヒー")),
                ai_token("を", Some("を")),
                ai_token("飲みます", Some("飲む")),
            ],
        };

        let left = sentence_similarity_signature(&stored);
        let right = candidate_similarity_signature(&candidate);
        assert!(is_highly_similar(&left, &right));
    }

    #[test]
    fn similarity_filter_keeps_distinct_content_words() {
        let stored = SentenceRecord {
            id: 1,
            lan: NativeLanguage::Chinese,
            source_text: "私は今日コーヒーを飲みます。".to_string(),
            translated_text: "我今天喝咖啡。".to_string(),
            style: None,
            created_at_unix_secs: 0,
            updated_at_unix_secs: 0,
            romaji_line: String::new(),
            furigana_line: String::new(),
            tokens: vec![
                token("私", Some("私")),
                token("今日", Some("今日")),
                token("コーヒー", Some("コーヒー")),
                token("飲みます", Some("飲む")),
            ],
            word_ids: vec![1],
            rewrite_status: RewriteStatus::Done,
            rewrite_error: None,
        };
        let candidate = AddAiResponse {
            japanese_sentence: "私は明日学校へ行きます。".to_string(),
            translated_sentence: "我明天去学校。".to_string(),
            romaji_line: String::new(),
            furigana_line: String::new(),
            tokens: vec![
                ai_token("私", Some("私")),
                ai_token("明日", Some("明日")),
                ai_token("学校", Some("学校")),
                ai_token("へ", Some("へ")),
                ai_token("行きます", Some("行く")),
            ],
        };

        let left = sentence_similarity_signature(&stored);
        let right = candidate_similarity_signature(&candidate);
        assert!(!is_highly_similar(&left, &right));
    }

    #[test]
    fn similarity_signature_handles_many_sentences_without_emptying() {
        let signatures = (0..200)
            .map(|index| SentenceRecord {
                id: index,
                lan: NativeLanguage::Chinese,
                source_text: format!("私は{}を勉強します。", index),
                translated_text: String::new(),
                style: None,
                created_at_unix_secs: 0,
                updated_at_unix_secs: 0,
                romaji_line: String::new(),
                furigana_line: String::new(),
                tokens: vec![
                    token("私", Some("私")),
                    token(&index.to_string(), Some(&index.to_string())),
                    token("勉強します", Some("勉強する")),
                ],
                word_ids: Vec::new(),
                rewrite_status: RewriteStatus::Done,
                rewrite_error: None,
            })
            .map(|sentence| sentence_similarity_signature(&sentence))
            .collect::<Vec<_>>();

        assert_eq!(signatures.len(), 200);
        assert!(signatures.iter().all(|signature| !signature.is_empty()));
    }

    fn token(surface: &str, lemma: Option<&str>) -> SentenceToken {
        SentenceToken {
            surface: surface.to_string(),
            reading: None,
            romaji: None,
            lemma: lemma.map(str::to_string),
            gloss: None,
            analysis: None,
            context_gloss: None,
            context_analysis: None,
            variants: vec![surface.to_string()],
            spans: vec![TokenSpan {
                text: surface.to_string(),
                reading: None,
            }],
        }
    }

    fn ai_token(surface: &str, lemma: Option<&str>) -> AddAiToken {
        AddAiToken {
            surface: surface.to_string(),
            reading: None,
            romaji: None,
            lemma: lemma.map(str::to_string),
            gloss: String::new(),
            analysis: String::new(),
            dictionary_gloss: None,
            dictionary_analysis: None,
            variants: vec![surface.to_string()],
            spans: vec![AddAiSpan {
                text: surface.to_string(),
                reading: None,
            }],
        }
    }
}
