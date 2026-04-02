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

const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy)]
enum OverlapMode {
    Balanced,
    High,
    Low,
}

#[test]
fn list_default_full_output_scales_cleanly_across_small_sizes() {
    let cases = [(64, 256), (128, 512), (256, 1024)];
    let mut results = Vec::new();

    for (sentence_count, word_count) in cases {
        let temp_home = TempDir::new().expect("temp home should exist");
        write_database(
            temp_home.path(),
            sentence_count,
            word_count,
            OverlapMode::Balanced,
        );
        warm_up_cli(temp_home.path());

        let timed = timed_run(temp_home.path(), &["list"]);
        let (duration, output) = timed;
        assert_success_within(
            &format!("list default full output ({sentence_count} sentences)"),
            &duration,
            &output,
            COMMAND_TIMEOUT,
        );
        let line_count = String::from_utf8(output.stdout)
            .expect("stdout should be utf8")
            .lines()
            .count();
        assert_eq!(line_count, sentence_count * 2);
        results.push((sentence_count, duration));
    }

    eprintln!("list default scaling timings: {results:?}");
}

#[test]
fn cat_performance_is_stable_for_high_and_low_overlap_distributions() {
    let scenarios = [
        ("high-overlap", OverlapMode::High, 180, 240, 80),
        ("low-overlap", OverlapMode::Low, 180, 720, 80),
    ];
    let mut timings = Vec::new();

    for (label, mode, sentence_count, word_count, review_count) in scenarios {
        let temp_home = TempDir::new().expect("temp home should exist");
        write_database(temp_home.path(), sentence_count, word_count, mode);
        warm_up_cli(temp_home.path());
        let timed = timed_run(temp_home.path(), &["cat", &review_count.to_string()]);
        let (duration, output) = timed;
        assert_success_within(&format!("cat {label}"), &duration, &output, COMMAND_TIMEOUT);
        let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
        assert!(stdout.contains('。'));
        timings.push((label, duration));
    }

    eprintln!("cat overlap timings: {timings:?}");
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

fn warm_up_cli(home: &Path) {
    let output = Command::new(env!("CARGO_BIN_EXE_nan"))
        .args(["list", "1", "sentence"])
        .env("HOME", home)
        .output()
        .expect("warm-up command should run");
    if !output.status.success() {
        panic!(
            "warm-up failed\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
}

fn assert_success_within(name: &str, duration: &Duration, output: &Output, limit: Duration) {
    if !output.status.success() {
        panic!(
            "{name} failed\nstatus: {:?}\nstdout: {}\nstderr: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
    assert!(
        *duration <= limit,
        "{name} exceeded time limit: {:?} > {:?}",
        duration,
        limit
    );
}

fn write_database(
    home: &Path,
    sentence_count: usize,
    word_count: usize,
    overlap_mode: OverlapMode,
) {
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
            let word_ids = generate_word_ids(index, sentence_count, word_count, overlap_mode);
            for word_id in &word_ids {
                database.words[*word_id as usize - 1]
                    .source_sentence_ids
                    .push(index as u64 + 1);
            }

            SentenceRecord {
                id: index as u64 + 1,
                lan: NativeLanguage::Chinese,
                source_text: format_sentence(&word_ids),
                translated_text: format_translation(&word_ids),
                style: None,
                created_at_unix_secs: index as i64,
                updated_at_unix_secs: index as i64,
                romaji_line: String::new(),
                furigana_line: String::new(),
                tokens: format_tokens(&word_ids),
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

fn generate_word_ids(
    index: usize,
    sentence_count: usize,
    word_count: usize,
    overlap_mode: OverlapMode,
) -> Vec<u64> {
    match overlap_mode {
        OverlapMode::Balanced => vec![
            (index % word_count) as u64 + 1,
            ((index + 1) % word_count) as u64 + 1,
            ((index + 2) % word_count) as u64 + 1,
            ((index + 3) % word_count) as u64 + 1,
        ],
        OverlapMode::High => {
            let variable = 4 + (index % word_count.saturating_sub(4).max(1));
            vec![1, 2, 3, variable as u64 + 1]
        }
        OverlapMode::Low => {
            let block = (index * 4) % word_count.max(sentence_count * 4);
            vec![
                (block % word_count) as u64 + 1,
                ((block + 1) % word_count) as u64 + 1,
                ((block + 2) % word_count) as u64 + 1,
                ((block + 3) % word_count) as u64 + 1,
            ]
        }
    }
}

fn format_sentence(word_ids: &[u64]) -> String {
    format!(
        "私は単語{}と単語{}と単語{}を勉強します。",
        word_ids[0] - 1,
        word_ids[1] - 1,
        word_ids[2] - 1,
    )
}

fn format_translation(word_ids: &[u64]) -> String {
    format!(
        "我学习词语{}、词语{}和词语{}。",
        word_ids[0] - 1,
        word_ids[1] - 1,
        word_ids[2] - 1,
    )
}

fn format_tokens(word_ids: &[u64]) -> Vec<SentenceToken> {
    vec![
        token("私", Some("私")),
        token(
            &format!("単語{}", word_ids[0] - 1),
            Some(&format!("単語{}", word_ids[0] - 1)),
        ),
        token(
            &format!("単語{}", word_ids[1] - 1),
            Some(&format!("単語{}", word_ids[1] - 1)),
        ),
        token(
            &format!("単語{}", word_ids[2] - 1),
            Some(&format!("単語{}", word_ids[2] - 1)),
        ),
        token("勉強します", Some("勉強する")),
        token("。", Some("。")),
    ]
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
