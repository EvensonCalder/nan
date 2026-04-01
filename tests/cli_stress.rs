use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use nan::model::{
    Database, NativeLanguage, RewriteStatus, SentenceRecord, SentenceToken, Settings, TokenSpan,
    WordRecord,
};
use nan::store::CONFIG_FILE_NAME;
use tempfile::TempDir;

const SENTENCE_COUNT: usize = 4_000;
const WORD_COUNT: usize = 800;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);

#[test]
fn cli_handles_large_database_without_pathological_slowdowns() {
    let temp_home = TempDir::new().expect("temp home should exist");
    write_large_database(temp_home.path(), SENTENCE_COUNT, WORD_COUNT);

    let list_sentence = timed_run(temp_home.path(), &["list", "200", "sentence"]);
    let list_word = timed_run(temp_home.path(), &["list", "200", "word"]);
    let cat = timed_run(temp_home.path(), &["cat", "200"]);
    let del = timed_run(temp_home.path(), &["del", "2000"]);

    eprintln!(
        "stress timings: list sentence={:?}, list word={:?}, cat={:?}, del={:?}",
        list_sentence.0, list_word.0, cat.0, del.0
    );

    assert_success_within("list sentence", list_sentence, COMMAND_TIMEOUT);
    assert_success_within("list word", list_word, COMMAND_TIMEOUT);
    assert_success_within("cat", cat, COMMAND_TIMEOUT);
    assert_success_within("del", del, COMMAND_TIMEOUT);

    let database = load_database(temp_home.path());
    assert_eq!(database.sentences.len(), SENTENCE_COUNT - 1);
}

fn timed_run(home: &Path, args: &[&str]) -> (Duration, Output) {
    let started = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_nan"))
        .args(args)
        .env("HOME", home)
        .output()
        .expect("nan command should run");
    (started.elapsed(), output)
}

fn assert_success_within(name: &str, timed: (Duration, Output), limit: Duration) {
    let (duration, output) = timed;
    if !output.status.success() {
        panic!(
            "{name} failed\nstatus: {:?}\nstdout: {}\nstderr: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
    assert!(
        duration <= limit,
        "{name} exceeded time limit: {:?} > {:?}",
        duration,
        limit
    );
}

fn write_large_database(home: &Path, sentence_count: usize, word_count: usize) {
    let settings = Settings {
        romaji_enabled: false,
        furigana_enabled: false,
        ..Settings::default()
    };

    let mut database = Database {
        settings,
        next_sentence_id: sentence_count as u64 + 1,
        next_word_id: word_count as u64 + 1,
        ..Database::default()
    };

    database.words = (0..word_count)
        .map(|index| WordRecord {
            id: index as u64 + 1,
            lan: NativeLanguage::Chinese,
            canonical_form: format!("単語{index}"),
            translation: format!("词语{index}"),
            analysis: format!("用于压力测试的词条 {index}"),
            variants: vec![format!("単語{index}")],
            source_sentence_ids: Vec::new(),
            s_last_days: 0.018 + (index % 10) as f64 * 0.01,
            t_last_unix_secs: index as i64,
            created_at_unix_secs: 0,
            updated_at_unix_secs: 0,
            rewrite_status: RewriteStatus::Done,
            rewrite_error: None,
        })
        .collect();

    database.sentences = (0..sentence_count)
        .map(|index| {
            let word_ids = vec![
                (index % word_count) as u64 + 1,
                ((index + 1) % word_count) as u64 + 1,
                ((index + 2) % word_count) as u64 + 1,
                ((index + 3) % word_count) as u64 + 1,
            ];
            for word_id in &word_ids {
                database.words[*word_id as usize - 1]
                    .source_sentence_ids
                    .push(index as u64 + 1);
            }

            SentenceRecord {
                id: index as u64 + 1,
                lan: NativeLanguage::Chinese,
                source_text: format!(
                    "私は単語{}と単語{}と単語{}を勉強します。",
                    index % word_count,
                    (index + 1) % word_count,
                    (index + 2) % word_count,
                ),
                translated_text: format!(
                    "我学习词语{}、词语{}和词语{}。",
                    index % word_count,
                    (index + 1) % word_count,
                    (index + 2) % word_count,
                ),
                style: None,
                created_at_unix_secs: index as i64,
                updated_at_unix_secs: index as i64,
                romaji_line: String::new(),
                furigana_line: String::new(),
                tokens: vec![
                    token("私", Some("私")),
                    token(
                        &format!("単語{}", index % word_count),
                        Some(&format!("単語{}", index % word_count)),
                    ),
                    token(
                        &format!("単語{}", (index + 1) % word_count),
                        Some(&format!("単語{}", (index + 1) % word_count)),
                    ),
                    token(
                        &format!("単語{}", (index + 2) % word_count),
                        Some(&format!("単語{}", (index + 2) % word_count)),
                    ),
                    token("勉強します", Some("勉強する")),
                    token("。", Some("。")),
                ],
                word_ids,
                rewrite_status: RewriteStatus::Done,
                rewrite_error: None,
            }
        })
        .collect();

    let path = home.join(CONFIG_FILE_NAME);
    fs::write(
        path,
        serde_json::to_string_pretty(&database).expect("database should serialize"),
    )
    .expect("database should write");
}

fn load_database(home: &Path) -> Database {
    let path = home.join(CONFIG_FILE_NAME);
    let content = fs::read_to_string(path).expect("config should exist");
    serde_json::from_str(&content).expect("config should parse")
}

fn token(surface: &str, lemma: Option<&str>) -> SentenceToken {
    SentenceToken {
        surface: surface.to_string(),
        reading: None,
        romaji: None,
        lemma: lemma.map(str::to_string),
        gloss: None,
        variants: vec![surface.to_string()],
        spans: vec![TokenSpan {
            text: surface.to_string(),
            reading: None,
        }],
    }
}
