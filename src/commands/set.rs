use crate::ai::{AiClient, SentenceRewriteAiResponse, WordRewriteAiResponse};
use crate::cli::{SetKey, parse_native_language, parse_proficiency_level};
use crate::error::NanError;
use crate::model::{
    Database, LanguageRewriteState, NativeLanguage, RewriteCursor, RewritePhase, RewriteStats,
    RewriteStatus,
};
use crate::prompt::{
    build_sentence_rewrite_prompt, build_word_rewrite_prompt, rewrite_system_prompt,
};
use crate::store::Store;

use super::add::current_unix_secs;

pub fn run(store: &Store, key: SetKey, option: String) -> Result<(), NanError> {
    let mut database = store.load_or_create()?;

    match key {
        SetKey::Ref => {
            let value = option
                .parse::<usize>()
                .map_err(|_| NanError::message("ref must be a positive integer"))?;
            if value == 0 {
                return Err(NanError::message("ref must be greater than 0"));
            }
            database.settings.ref_capacity = value;
            store.save(&database)?;
            println!("ref set to {value}");
            Ok(())
        }
        SetKey::Level => {
            let level = parse_proficiency_level(option.trim())?;
            database.settings.level = level;
            store.save(&database)?;
            println!("level set to {}", level.as_str());
            Ok(())
        }
        SetKey::BaseUrl => {
            let value = option.trim();
            if value.is_empty() {
                return Err(NanError::message("base-url must not be empty"));
            }
            database.settings.base_url = value.to_string();
            store.save(&database)?;
            println!("base-url updated");
            Ok(())
        }
        SetKey::ApiKey => {
            let value = option.trim();
            if value.is_empty() {
                return Err(NanError::message("api-key must not be empty"));
            }
            database.settings.api_key = Some(value.to_string());
            store.save(&database)?;
            println!("api-key updated");
            Ok(())
        }
        SetKey::Model => {
            let value = option.trim();
            if value.is_empty() {
                return Err(NanError::message("model must not be empty"));
            }
            database.settings.model = value.to_string();
            store.save(&database)?;
            println!("model set to {value}");
            Ok(())
        }
        SetKey::Roomaji => {
            let value = parse_toggle(option.trim())?;
            database.settings.romaji_enabled = value;
            store.save(&database)?;
            println!("roomaji set to {}", on_off(value));
            Ok(())
        }
        SetKey::Furigana => {
            let value = parse_toggle(option.trim())?;
            database.settings.furigana_enabled = value;
            store.save(&database)?;
            println!("furigana set to {}", on_off(value));
            Ok(())
        }
        SetKey::Lan => {
            let target_language = parse_native_language(option.trim())?;
            rewrite_language(store, target_language)
        }
    }
}

pub(crate) fn has_language_mismatch(database: &Database) -> bool {
    database
        .sentences
        .iter()
        .any(|sentence| sentence.lan != database.settings.lan)
        || database
            .words
            .iter()
            .any(|word| word.lan != database.settings.lan)
        || database.language_rewrite.is_some()
}

pub(crate) fn rewrite_language(
    store: &Store,
    target_language: NativeLanguage,
) -> Result<(), NanError> {
    let mut database = store.load_or_create()?;
    let now_unix_secs = current_unix_secs()?;
    prepare_language_rewrite(&mut database, target_language, now_unix_secs);
    store.save(&database)?;

    if database.sentences.is_empty() && database.words.is_empty() {
        database.language_rewrite = None;
        store.save(&database)?;
        println!("language set to {}", target_language.as_str());
        return Ok(());
    }

    let settings = database.settings.clone();
    let client = AiClient::from_settings(&settings)?;
    rewrite_sentences(store, &client, database)
}

fn prepare_language_rewrite(
    database: &mut Database,
    target_language: NativeLanguage,
    now_unix_secs: i64,
) {
    let previous_language = database.settings.lan;
    database.settings.lan = target_language;

    for sentence in &mut database.sentences {
        if sentence.lan != target_language {
            sentence.rewrite_status = RewriteStatus::Pending;
            sentence.rewrite_error = None;
        }
    }
    for word in &mut database.words {
        if word.lan != target_language {
            word.rewrite_status = RewriteStatus::Pending;
            word.rewrite_error = None;
        }
    }

    database.language_rewrite = Some(LanguageRewriteState {
        from_lan: previous_language,
        to_lan: target_language,
        started_at_unix_secs: now_unix_secs,
        updated_at_unix_secs: now_unix_secs,
        cursor: RewriteCursor {
            phase: RewritePhase::Sentences,
            index: 0,
        },
        stats: rebuild_rewrite_stats(database, target_language),
        last_error: None,
    });
}

fn rewrite_sentences(
    store: &Store,
    client: &AiClient,
    mut database: Database,
) -> Result<(), NanError> {
    let target_language = database.settings.lan;

    for index in 0..database.sentences.len() {
        if database.sentences[index].lan == target_language {
            continue;
        }

        let prompt = build_sentence_rewrite_prompt(
            &database.sentences[index].source_text,
            &database.sentences[index].translated_text,
            target_language,
        );
        let response: SentenceRewriteAiResponse =
            match client.chat_json(rewrite_system_prompt(), &prompt) {
                Ok(response) => response,
                Err(error) => {
                    database.sentences[index].rewrite_status = RewriteStatus::Failed;
                    database.sentences[index].rewrite_error = Some(error.to_string());
                    update_rewrite_error(&mut database, error.to_string(), target_language)?;
                    store.save(&database)?;
                    return Err(error);
                }
            };
        database.sentences[index].translated_text = response.translated_sentence;
        database.sentences[index].lan = target_language;
        database.sentences[index].rewrite_status = RewriteStatus::Done;
        database.sentences[index].rewrite_error = None;
        database.sentences[index].updated_at_unix_secs = current_unix_secs()?;

        let stats = rebuild_rewrite_stats(&database, target_language);
        if let Some(rewrite) = &mut database.language_rewrite {
            rewrite.cursor.phase = RewritePhase::Sentences;
            rewrite.cursor.index = index + 1;
            rewrite.updated_at_unix_secs = current_unix_secs()?;
            rewrite.stats = stats;
        }
        store.save(&database)?;
    }

    for index in 0..database.words.len() {
        if database.words[index].lan == target_language {
            continue;
        }

        let prompt = build_word_rewrite_prompt(
            &database.words[index].canonical_form,
            &database.words[index].translation,
            &database.words[index].analysis,
            target_language,
        );
        let response: WordRewriteAiResponse =
            match client.chat_json(rewrite_system_prompt(), &prompt) {
                Ok(response) => response,
                Err(error) => {
                    database.words[index].rewrite_status = RewriteStatus::Failed;
                    database.words[index].rewrite_error = Some(error.to_string());
                    update_rewrite_error(&mut database, error.to_string(), target_language)?;
                    store.save(&database)?;
                    return Err(error);
                }
            };
        database.words[index].translation = response.translation;
        database.words[index].analysis = response.analysis;
        database.words[index].lan = target_language;
        database.words[index].rewrite_status = RewriteStatus::Done;
        database.words[index].rewrite_error = None;
        database.words[index].updated_at_unix_secs = current_unix_secs()?;

        let stats = rebuild_rewrite_stats(&database, target_language);
        if let Some(rewrite) = &mut database.language_rewrite {
            rewrite.cursor.phase = RewritePhase::Words;
            rewrite.cursor.index = index + 1;
            rewrite.updated_at_unix_secs = current_unix_secs()?;
            rewrite.stats = stats;
        }
        store.save(&database)?;
    }

    database.language_rewrite = None;
    store.save(&database)?;
    println!("language set to {}", target_language.as_str());
    Ok(())
}

fn rebuild_rewrite_stats(database: &Database, target_language: NativeLanguage) -> RewriteStats {
    RewriteStats {
        total_sentences: database.sentences.len(),
        done_sentences: database
            .sentences
            .iter()
            .filter(|sentence| sentence.lan == target_language)
            .count(),
        total_words: database.words.len(),
        done_words: database
            .words
            .iter()
            .filter(|word| word.lan == target_language)
            .count(),
        failures: database
            .sentences
            .iter()
            .filter(|sentence| matches!(sentence.rewrite_status, RewriteStatus::Failed))
            .count()
            + database
                .words
                .iter()
                .filter(|word| matches!(word.rewrite_status, RewriteStatus::Failed))
                .count(),
    }
}

fn update_rewrite_error(
    database: &mut Database,
    message: String,
    target_language: NativeLanguage,
) -> Result<(), NanError> {
    let stats = rebuild_rewrite_stats(database, target_language);
    if let Some(rewrite) = &mut database.language_rewrite {
        rewrite.updated_at_unix_secs = current_unix_secs()?;
        rewrite.last_error = Some(message);
        rewrite.stats = stats;
    }
    Ok(())
}

fn parse_toggle(value: &str) -> Result<bool, NanError> {
    match value {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err(NanError::message("option must be `on` or `off`")),
    }
}

fn on_off(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

#[cfg(test)]
mod tests {
    use crate::model::{Database, NativeLanguage, RewriteStatus, SentenceRecord, WordRecord};

    use super::{has_language_mismatch, prepare_language_rewrite};

    #[test]
    fn mismatch_is_detected_from_records() {
        let mut database = Database::default();
        database.sentences.push(SentenceRecord {
            id: 1,
            lan: NativeLanguage::English,
            source_text: "今夜は月がきれいですね。".to_string(),
            translated_text: "Tonight the moon is beautiful.".to_string(),
            style: None,
            created_at_unix_secs: 0,
            updated_at_unix_secs: 0,
            romaji_line: String::new(),
            furigana_line: String::new(),
            tokens: Vec::new(),
            word_ids: vec![1],
            rewrite_status: RewriteStatus::Done,
            rewrite_error: None,
        });
        assert!(has_language_mismatch(&database));
    }

    #[test]
    fn prepare_rewrite_marks_non_target_records_pending() {
        let mut database = Database::default();
        database.words.push(WordRecord {
            id: 1,
            lan: NativeLanguage::English,
            canonical_form: "食べる".to_string(),
            translation: "eat".to_string(),
            analysis: "to eat".to_string(),
            variants: vec!["食べる".to_string()],
            source_sentence_ids: vec![1],
            s_last_days: 1.0,
            t_last_unix_secs: 0,
            created_at_unix_secs: 0,
            updated_at_unix_secs: 0,
            rewrite_status: RewriteStatus::Done,
            rewrite_error: None,
        });

        prepare_language_rewrite(&mut database, NativeLanguage::Chinese, 10);
        assert_eq!(database.settings.lan, NativeLanguage::Chinese);
        assert_eq!(database.words[0].rewrite_status, RewriteStatus::Pending);
        assert!(database.language_rewrite.is_some());
    }
}
