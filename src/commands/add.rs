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
    let _lock = store.lock()?;
    let mut database = store.load_or_create_unlocked()?;
    let settings = database.settings.clone();
    let client = AiClient::from_settings(&settings)?;
    let prompt = build_add_user_prompt(&sentence, style.as_deref(), settings.level, settings.lan);
    let response: AddAiResponse = client.chat_json(add_system_prompt(), &prompt)?;
    let now_unix_secs = current_unix_secs()?;
    let sentence_record = insert_sentence(&mut database, response, style, now_unix_secs);
    store.save_unlocked(&database)?;

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
            .map(|token| {
                let (gloss, analysis) = find_matching_word(database, token)
                    .map(|word| (Some(word.translation.clone()), Some(word.analysis.clone())))
                    .unwrap_or_else(|| {
                        (
                            Some(resolve_word_translation(token)),
                            Some(resolve_word_analysis(token)),
                        )
                    });

                SentenceToken {
                    surface: token.surface.clone(),
                    reading: token.reading.clone(),
                    romaji: token.romaji.clone(),
                    lemma: token.lemma.clone(),
                    gloss,
                    analysis,
                    context_gloss: Some(token.gloss.clone()),
                    context_analysis: Some(token.analysis.clone()),
                    variants: token.variants.clone(),
                    spans: token
                        .spans
                        .iter()
                        .map(|span| TokenSpan {
                            text: span.text.clone(),
                            reading: span.reading.clone(),
                        })
                        .collect(),
                }
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
    let target_language = database.settings.lan;
    if let Some(existing_word) = find_matching_word_mut(database, token) {
        existing_word.lan = target_language;
        if let Some(translation) = token_dictionary_translation(token) {
            existing_word.translation = translation;
        }
        if let Some(analysis) = token_dictionary_analysis(token) {
            existing_word.analysis = analysis;
        }
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
        lan: target_language,
        canonical_form: token.lemma.clone().unwrap_or_else(|| token.surface.clone()),
        translation: resolve_word_translation(token),
        analysis: resolve_word_analysis(token),
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

fn resolve_word_translation(token: &crate::ai::AddAiToken) -> String {
    token_dictionary_translation(token).unwrap_or_else(|| token.gloss.trim().to_string())
}

fn resolve_word_analysis(token: &crate::ai::AddAiToken) -> String {
    token_dictionary_analysis(token).unwrap_or_else(|| token.analysis.trim().to_string())
}

fn token_lookup_values(token: &crate::ai::AddAiToken) -> Vec<String> {
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
    lookup_values
}

fn word_lookup_keys(word: &WordRecord) -> Vec<String> {
    word.variants
        .iter()
        .map(|variant| normalize_word_key(variant))
        .chain(std::iter::once(normalize_word_key(&word.canonical_form)))
        .collect()
}

fn find_matching_word<'a>(
    database: &'a Database,
    token: &crate::ai::AddAiToken,
) -> Option<&'a WordRecord> {
    let lookup_values = token_lookup_values(token);
    database.words.iter().find(|word| {
        let existing_keys = word_lookup_keys(word);
        lookup_values
            .iter()
            .any(|candidate| existing_keys.contains(candidate))
    })
}

fn find_matching_word_mut<'a>(
    database: &'a mut Database,
    token: &crate::ai::AddAiToken,
) -> Option<&'a mut WordRecord> {
    let lookup_values = token_lookup_values(token);
    database.words.iter_mut().find(|word| {
        let existing_keys = word_lookup_keys(word);
        lookup_values
            .iter()
            .any(|candidate| existing_keys.contains(candidate))
    })
}

fn token_dictionary_translation(token: &crate::ai::AddAiToken) -> Option<String> {
    token
        .dictionary_gloss
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn token_dictionary_analysis(token: &crate::ai::AddAiToken) -> Option<String> {
    token
        .dictionary_analysis
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
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
                dictionary_gloss: Some("eat".to_string()),
                dictionary_analysis: Some("dictionary form of to eat".to_string()),
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
                    dictionary_gloss: Some("猫".to_string()),
                    dictionary_analysis: Some("名词".to_string()),
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
                    dictionary_gloss: Some("句号".to_string()),
                    dictionary_analysis: Some("标点".to_string()),
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

    #[test]
    fn insert_sentence_keeps_dictionary_meaning_separate_from_context_meaning() {
        let mut database = Database::default();
        let response = AddAiResponse {
            japanese_sentence: "私は朝コーヒーを飲みません。".to_string(),
            translated_sentence: "我早上不喝咖啡。".to_string(),
            romaji_line: "watashi wa asa koohii o nomimasen.".to_string(),
            furigana_line: "私[わたし]は朝[あさ]コーヒーを飲[の]みません。".to_string(),
            tokens: vec![AddAiToken {
                surface: "飲みません".to_string(),
                reading: Some("のみません".to_string()),
                romaji: Some("nomimasen".to_string()),
                lemma: Some("飲む".to_string()),
                gloss: "不喝".to_string(),
                analysis: "礼貌否定形动词".to_string(),
                dictionary_gloss: Some("喝".to_string()),
                dictionary_analysis: Some("动词原形，表示喝".to_string()),
                variants: vec!["飲みません".to_string(), "飲む".to_string()],
                spans: vec![AddAiSpan {
                    text: "飲みません".to_string(),
                    reading: Some("のみません".to_string()),
                }],
            }],
        };

        let sentence = insert_sentence(&mut database, response, None, 100);
        assert_eq!(database.words[0].canonical_form, "飲む");
        assert_eq!(database.words[0].translation, "喝");
        assert_eq!(database.words[0].analysis, "动词原形，表示喝");
        assert_eq!(sentence.tokens[0].gloss.as_deref(), Some("喝"));
        assert_eq!(sentence.tokens[0].context_gloss.as_deref(), Some("不喝"));
        assert_eq!(
            sentence.tokens[0].context_analysis.as_deref(),
            Some("礼貌否定形动词")
        );
    }

    #[test]
    fn insert_sentence_does_not_overwrite_existing_dictionary_meaning_without_dictionary_fields() {
        let mut database = Database::default();
        let first = AddAiResponse {
            japanese_sentence: "私はコーヒーを飲みます。".to_string(),
            translated_sentence: "我喝咖啡。".to_string(),
            romaji_line: "watashi wa koohii o nomimasu.".to_string(),
            furigana_line: "私[わたし]は コーヒーを 飲[の]みます。".to_string(),
            tokens: vec![AddAiToken {
                surface: "飲みます".to_string(),
                reading: Some("のみます".to_string()),
                romaji: Some("nomimasu".to_string()),
                lemma: Some("飲む".to_string()),
                gloss: "喝".to_string(),
                analysis: "礼貌形动词".to_string(),
                dictionary_gloss: Some("喝".to_string()),
                dictionary_analysis: Some("动词原形，表示喝".to_string()),
                variants: vec!["飲みます".to_string(), "飲む".to_string()],
                spans: vec![AddAiSpan {
                    text: "飲みます".to_string(),
                    reading: Some("のみます".to_string()),
                }],
            }],
        };
        let second = AddAiResponse {
            japanese_sentence: "私はコーヒーを飲みません。".to_string(),
            translated_sentence: "我不喝咖啡。".to_string(),
            romaji_line: "watashi wa koohii o nomimasen.".to_string(),
            furigana_line: "私[わたし]は コーヒーを 飲[の]みません。".to_string(),
            tokens: vec![AddAiToken {
                surface: "飲みません".to_string(),
                reading: Some("のみません".to_string()),
                romaji: Some("nomimasen".to_string()),
                lemma: Some("飲む".to_string()),
                gloss: "不喝".to_string(),
                analysis: "礼貌否定形动词".to_string(),
                dictionary_gloss: None,
                dictionary_analysis: None,
                variants: vec!["飲みません".to_string(), "飲む".to_string()],
                spans: vec![AddAiSpan {
                    text: "飲みません".to_string(),
                    reading: Some("のみません".to_string()),
                }],
            }],
        };

        insert_sentence(&mut database, first, None, 100);
        insert_sentence(&mut database, second, None, 200);
        assert_eq!(database.words[0].translation, "喝");
        assert_eq!(database.words[0].analysis, "动词原形，表示喝");
    }
}
