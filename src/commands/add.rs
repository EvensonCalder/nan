use std::time::{SystemTime, UNIX_EPOCH};

use crate::ai::{AddAiResponse, AiClient};
use crate::error::NanError;
use crate::model::{
    Database, RewriteStatus, SentenceRecord, SentenceToken, TokenSpan, WordRecord,
    is_japanese_punctuation, normalize_word_key,
};
use crate::prompt::{add_system_prompt, build_add_user_prompt};
use crate::render::render_sentence;
use crate::review::INITIAL_STABILITY_DAYS;
use crate::store::Store;

pub fn run(store: &Store, sentence: String, style: Option<String>) -> Result<(), NanError> {
    let mut database = store.load_or_create()?;
    let settings = database.settings.clone();
    let client = AiClient::from_settings(&settings)?;
    let prompt = build_add_user_prompt(&sentence, style.as_deref(), settings.level, settings.lan);
    let response: AddAiResponse = client.chat_json(add_system_prompt(), &prompt)?;
    let now_unix_secs = current_unix_secs()?;
    let sentence_record = insert_sentence(&mut database, response, style, now_unix_secs);
    store.save(&database)?;

    println!("{}", render_sentence(&sentence_record, &settings));
    Ok(())
}

pub(crate) fn insert_sentence(
    database: &mut Database,
    response: AddAiResponse,
    style: Option<String>,
    now_unix_secs: i64,
) -> SentenceRecord {
    let mut word_ids = Vec::new();

    for token in &response.tokens {
        if is_japanese_punctuation(&token.surface) {
            continue;
        }
        let word_id = upsert_word(database, token, now_unix_secs);
        if !word_ids.contains(&word_id) {
            word_ids.push(word_id);
        }
    }

    let sentence_record = SentenceRecord {
        id: database.allocate_sentence_id(),
        lan: database.settings.lan,
        source_text: response.japanese_sentence,
        translated_text: response.translated_sentence,
        style,
        created_at_unix_secs: now_unix_secs,
        updated_at_unix_secs: now_unix_secs,
        romaji_line: response.romaji_line,
        furigana_line: response.furigana_line,
        tokens: response
            .tokens
            .iter()
            .map(|token| SentenceToken {
                surface: token.surface.clone(),
                reading: token.reading.clone(),
                romaji: token.romaji.clone(),
                lemma: token.lemma.clone(),
                gloss: Some(token.gloss.clone()),
                variants: token.variants.clone(),
                spans: token
                    .spans
                    .iter()
                    .map(|span| TokenSpan {
                        text: span.text.clone(),
                        reading: span.reading.clone(),
                    })
                    .collect(),
            })
            .collect(),
        word_ids: word_ids.clone(),
        rewrite_status: RewriteStatus::Done,
        rewrite_error: None,
    };

    let sentence_id = sentence_record.id;
    for word in &mut database.words {
        if word_ids.contains(&word.id) && !word.source_sentence_ids.contains(&sentence_id) {
            word.source_sentence_ids.push(sentence_id);
            word.updated_at_unix_secs = now_unix_secs;
        }
    }

    database.sentences.push(sentence_record.clone());
    sentence_record
}

fn upsert_word(database: &mut Database, token: &crate::ai::AddAiToken, now_unix_secs: i64) -> u64 {
    let mut lookup_values: Vec<String> = token
        .variants
        .iter()
        .map(|variant| normalize_word_key(variant))
        .filter(|variant| !variant.is_empty())
        .collect();

    lookup_values.push(normalize_word_key(&token.surface));
    if let Some(lemma) = &token.lemma {
        lookup_values.push(normalize_word_key(lemma));
    }

    lookup_values.sort();
    lookup_values.dedup();

    if let Some(existing_word) = database.words.iter_mut().find(|word| {
        let existing_keys = word
            .variants
            .iter()
            .map(|variant| normalize_word_key(variant))
            .chain(std::iter::once(normalize_word_key(&word.canonical_form)))
            .collect::<Vec<_>>();

        lookup_values
            .iter()
            .any(|candidate| existing_keys.contains(candidate))
    }) {
        existing_word.lan = database.settings.lan;
        existing_word.translation = token.gloss.clone();
        existing_word.analysis = token.analysis.clone();
        existing_word.updated_at_unix_secs = now_unix_secs;
        existing_word.rewrite_status = RewriteStatus::Done;
        existing_word.rewrite_error = None;

        for variant in &token.variants {
            if !existing_word.variants.contains(variant) {
                existing_word.variants.push(variant.clone());
            }
        }

        if !existing_word.variants.contains(&token.surface) {
            existing_word.variants.push(token.surface.clone());
        }

        return existing_word.id;
    }

    let word = WordRecord {
        id: database.allocate_word_id(),
        lan: database.settings.lan,
        canonical_form: token.lemma.clone().unwrap_or_else(|| token.surface.clone()),
        translation: token.gloss.clone(),
        analysis: token.analysis.clone(),
        variants: {
            let mut variants = token.variants.clone();
            if !variants.contains(&token.surface) {
                variants.push(token.surface.clone());
            }
            variants
        },
        source_sentence_ids: Vec::new(),
        s_last_days: INITIAL_STABILITY_DAYS,
        t_last_unix_secs: now_unix_secs,
        created_at_unix_secs: now_unix_secs,
        updated_at_unix_secs: now_unix_secs,
        rewrite_status: RewriteStatus::Done,
        rewrite_error: None,
    };
    let word_id = word.id;
    database.words.push(word);
    word_id
}

pub(crate) fn current_unix_secs() -> Result<i64, NanError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            NanError::message(format!("system clock is before UNIX_EPOCH: {error}"))
        })?;

    i64::try_from(duration.as_secs())
        .map_err(|_| NanError::message("current system time does not fit into i64"))
}

#[cfg(test)]
mod tests {
    use crate::ai::{AddAiResponse, AddAiSpan, AddAiToken};
    use crate::model::Database;

    use super::insert_sentence;

    #[test]
    fn insert_sentence_reuses_existing_word_variants() {
        let mut database = Database::default();
        let response = AddAiResponse {
            japanese_sentence: "食べます。".to_string(),
            translated_sentence: "To eat.".to_string(),
            romaji_line: "tabemasu.".to_string(),
            furigana_line: "たべます。".to_string(),
            tokens: vec![AddAiToken {
                surface: "食べます".to_string(),
                reading: Some("たべます".to_string()),
                romaji: Some("tabemasu".to_string()),
                lemma: Some("食べる".to_string()),
                gloss: "eat".to_string(),
                analysis: "polite present form of to eat".to_string(),
                variants: vec!["食べます".to_string(), "食べる".to_string()],
                spans: vec![AddAiSpan {
                    text: "食".to_string(),
                    reading: Some("た".to_string()),
                }],
            }],
        };

        let first = insert_sentence(&mut database, response.clone(), None, 100).word_ids;
        let second = insert_sentence(&mut database, response, None, 200).word_ids;
        assert_eq!(database.words.len(), 1);
        assert_eq!(first, second);
        assert_eq!(database.sentences.len(), 2);
        assert_eq!(database.words[0].source_sentence_ids.len(), 2);
    }

    #[test]
    fn insert_sentence_skips_punctuation_tokens_for_word_storage() {
        let mut database = Database::default();
        let response = AddAiResponse {
            japanese_sentence: "猫です。".to_string(),
            translated_sentence: "这是猫。".to_string(),
            romaji_line: "neko desu.".to_string(),
            furigana_line: "ねこ です。".to_string(),
            tokens: vec![
                AddAiToken {
                    surface: "猫".to_string(),
                    reading: Some("ねこ".to_string()),
                    romaji: Some("neko".to_string()),
                    lemma: Some("猫".to_string()),
                    gloss: "猫".to_string(),
                    analysis: "名词".to_string(),
                    variants: vec!["猫".to_string()],
                    spans: vec![AddAiSpan {
                        text: "猫".to_string(),
                        reading: Some("ねこ".to_string()),
                    }],
                },
                AddAiToken {
                    surface: "。".to_string(),
                    reading: None,
                    romaji: None,
                    lemma: None,
                    gloss: "句号".to_string(),
                    analysis: "标点".to_string(),
                    variants: vec!["。".to_string()],
                    spans: vec![AddAiSpan {
                        text: "。".to_string(),
                        reading: None,
                    }],
                },
            ],
        };

        let sentence = insert_sentence(&mut database, response, None, 100);
        assert_eq!(database.words.len(), 1);
        assert_eq!(sentence.word_ids.len(), 1);
    }
}
