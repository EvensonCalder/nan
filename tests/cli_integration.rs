use std::collections::VecDeque;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use nan::model::{CURRENT_SCHEMA_VERSION, Database};
use nan::store::CONFIG_FILE_NAME;
use serde_json::{Value, json};
use tempfile::TempDir;

const TEST_API_KEY: &str = "integration-test-key";
const TEST_MODEL: &str = "integration-test-model";
const MIGRATION_STATE_FILE_NAME: &str = ".nanconfig.json.migration-state.json";
const MIGRATION_BACKUP_FILE_NAME: &str = ".nanconfig.json.migration-backup.json";

#[test]
fn add_uses_environment_configuration_and_persists_annotations_when_hidden() {
    let temp_home = TempDir::new().expect("temp home should exist");
    let server = MockServer::start(vec![MockResponse::json(success_body(json!({
        "japanese_sentence": "私は今日コーヒーを飲みます。",
        "translated_sentence": "我今天喝咖啡。",
        "romaji_line": "watashi wa kyou koohii o nomimasu.",
        "furigana_line": "私[わたし]は 今日[きょう] コーヒーを 飲[の]みます。",
        "tokens": [
            token_json("私", Some("私"), Some("わたし"), Some("watashi"), "我", "第一人称代词", ["私", "わたし"]),
            token_json("は", Some("は"), Some("は"), Some("wa"), "主题标记", "主题助词", ["は"]),
            token_json("今日", Some("今日"), Some("きょう"), Some("kyou"), "今天", "时间名词", ["今日", "きょう"]),
            token_json("コーヒー", Some("コーヒー"), Some("コーヒー"), Some("koohii"), "咖啡", "外来语名词", ["コーヒー"]),
            token_json("を", Some("を"), Some("を"), Some("o"), "宾语标记", "宾语助词", ["を"]),
            token_json("飲みます", Some("飲む"), Some("のみます"), Some("nomimasu"), "喝", "礼貌形动词", ["飲みます", "飲む"]),
            token_json("。", Some("。"), None, None, "句号", "句末标点", ["。"])
        ]
    })))]);

    assert_success(run_nan(
        temp_home.path(),
        &server.base_url,
        &["set", "roomaji", "off"],
    ));
    assert_success(run_nan(
        temp_home.path(),
        &server.base_url,
        &["set", "furigana", "off"],
    ));

    let output = assert_success(run_nan(
        temp_home.path(),
        &server.base_url,
        &["add", "我今天喝咖啡"],
    ));
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(stdout.contains("我今天喝咖啡。"));
    assert!(stdout.contains("私"));
    assert!(stdout.contains("コーヒー"));
    assert!(stdout.contains("飲みます"));
    assert!(!stdout.contains("watashi wa kyou"));
    assert!(!stdout.contains("わたし"));

    let database = load_database(temp_home.path());
    assert_eq!(database.sentences.len(), 1);
    assert_eq!(
        database.sentences[0].romaji_line,
        "watashi wa kyou koohii o nomimasu."
    );
    assert_eq!(
        database.sentences[0].furigana_line,
        "私[わたし]は 今日[きょう] コーヒーを 飲[の]みます。"
    );
    assert_eq!(
        database.sentences[0].tokens[0].romaji.as_deref(),
        Some("watashi")
    );

    let requests = server.finish();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["model"], TEST_MODEL);
}

#[test]
fn list_auto_migrates_legacy_database_before_running() {
    let temp_home = TempDir::new().expect("temp home should exist");
    write_legacy_v1_database(temp_home.path());

    let output = assert_success(run_nan(
        temp_home.path(),
        "http://127.0.0.1:1",
        &["list", "1", "word"],
    ));
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(stdout.contains("飲む"));
    assert!(stdout.contains("喝"));

    let database = load_database(temp_home.path());
    assert_eq!(database.schema_version, CURRENT_SCHEMA_VERSION);
    assert_eq!(database.sentences[0].tokens[0].gloss.as_deref(), Some("喝"));
    assert_eq!(
        database.sentences[0].tokens[0].context_gloss.as_deref(),
        Some("不喝")
    );
    assert!(!temp_home.path().join(MIGRATION_STATE_FILE_NAME).exists());
    assert!(!temp_home.path().join(MIGRATION_BACKUP_FILE_NAME).exists());
}

#[test]
fn list_recovers_interrupted_migration_before_running() {
    let temp_home = TempDir::new().expect("temp home should exist");
    let legacy = legacy_v1_database_json().to_string();
    fs::write(temp_home.path().join(MIGRATION_BACKUP_FILE_NAME), legacy)
        .expect("migration backup should write");
    fs::write(
        temp_home.path().join(MIGRATION_STATE_FILE_NAME),
        json!({"from_version": 1, "target_version": CURRENT_SCHEMA_VERSION}).to_string(),
    )
    .expect("migration state should write");

    assert_success(run_nan(
        temp_home.path(),
        "http://127.0.0.1:1",
        &["list", "1", "word"],
    ));

    let database = load_database(temp_home.path());
    assert_eq!(database.schema_version, CURRENT_SCHEMA_VERSION);
    assert_eq!(database.words.len(), 1);
    assert_eq!(database.words[0].canonical_form, "飲む");
    assert_eq!(database.words[0].translation, "喝");
    assert_eq!(
        database.sentences[0].tokens[0].context_gloss.as_deref(),
        Some("不喝")
    );
    assert!(!temp_home.path().join(MIGRATION_STATE_FILE_NAME).exists());
    assert!(!temp_home.path().join(MIGRATION_BACKUP_FILE_NAME).exists());
}

#[test]
fn add_renders_cleanly_when_only_romaji_is_hidden() {
    let temp_home = TempDir::new().expect("temp home should exist");
    let server = MockServer::start(vec![MockResponse::json(success_body(json!({
        "japanese_sentence": "匿名さん、すごいですね。",
        "translated_sentence": "匿名的人真厉害啊。",
        "romaji_line": "tokumei-san sugoi desu ne",
        "furigana_line": "とくめいさん すごい です ね",
        "tokens": [
            token_json("匿名さん", Some("匿名さん"), Some("とくめいさん"), Some("tokumei-san"), "匿名的人", "称呼匿名的人", ["匿名さん"]),
            token_json("、", Some("、"), None, None, "逗号", "标点", ["、"]),
            token_json("すごい", Some("すごい"), Some("すごい"), Some("sugoi"), "厉害", "形容词", ["すごい"]),
            token_json("です", Some("です"), Some("です"), Some("desu"), "是", "礼貌语尾", ["です"]),
            token_json("ね", Some("ね"), Some("ね"), Some("ne"), "呢", "句末助词", ["ね"]),
            token_json("。", Some("。"), None, None, "句号", "标点", ["。"])
        ]
    })))]);

    assert_success(run_nan(
        temp_home.path(),
        &server.base_url,
        &["set", "roomaji", "off"],
    ));
    let output = assert_success(run_nan(
        temp_home.path(),
        &server.base_url,
        &["add", "匿名的人真厉害啊"],
    ));
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(!stdout.contains("tokumei-san"));
    assert!(stdout.contains("とくめいさん"));
    assert!(stdout.contains("匿名さん"));

    server.finish();
}

#[test]
fn add_renders_cleanly_when_only_furigana_is_hidden() {
    let temp_home = TempDir::new().expect("temp home should exist");
    let server = MockServer::start(vec![MockResponse::json(success_body(json!({
        "japanese_sentence": "匿名さん、すごいですね。",
        "translated_sentence": "匿名的人真厉害啊。",
        "romaji_line": "tokumei-san sugoi desu ne",
        "furigana_line": "とくめいさん すごい です ね",
        "tokens": [
            token_json("匿名さん", Some("匿名さん"), Some("とくめいさん"), Some("tokumei-san"), "匿名的人", "称呼匿名的人", ["匿名さん"]),
            token_json("、", Some("、"), None, None, "逗号", "标点", ["、"]),
            token_json("すごい", Some("すごい"), Some("すごい"), Some("sugoi"), "厉害", "形容词", ["すごい"]),
            token_json("です", Some("です"), Some("です"), Some("desu"), "是", "礼貌语尾", ["です"]),
            token_json("ね", Some("ね"), Some("ね"), Some("ne"), "呢", "句末助词", ["ね"]),
            token_json("。", Some("。"), None, None, "句号", "标点", ["。"])
        ]
    })))]);

    assert_success(run_nan(
        temp_home.path(),
        &server.base_url,
        &["set", "furigana", "off"],
    ));
    let output = assert_success(run_nan(
        temp_home.path(),
        &server.base_url,
        &["add", "匿名的人真厉害啊"],
    ));
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(stdout.contains("tokumei-san"));
    assert!(!stdout.contains("とくめいさん"));
    assert!(stdout.contains("匿名さん"));

    server.finish();
}

#[test]
fn add_renders_cleanly_when_both_romaji_and_furigana_are_hidden() {
    let temp_home = TempDir::new().expect("temp home should exist");
    let server = MockServer::start(vec![MockResponse::json(success_body(json!({
        "japanese_sentence": "匿名さん、すごいですね。",
        "translated_sentence": "匿名的人真厉害啊。",
        "romaji_line": "tokumei-san sugoi desu ne",
        "furigana_line": "とくめいさん すごい です ね",
        "tokens": [
            token_json("匿名さん", Some("匿名さん"), Some("とくめいさん"), Some("tokumei-san"), "匿名的人", "称呼匿名的人", ["匿名さん"]),
            token_json("、", Some("、"), None, None, "逗号", "标点", ["、"]),
            token_json("すごい", Some("すごい"), Some("すごい"), Some("sugoi"), "厉害", "形容词", ["すごい"]),
            token_json("です", Some("です"), Some("です"), Some("desu"), "是", "礼貌语尾", ["です"]),
            token_json("ね", Some("ね"), Some("ね"), Some("ne"), "呢", "句末助词", ["ね"]),
            token_json("。", Some("。"), None, None, "句号", "标点", ["。"])
        ]
    })))]);

    assert_success(run_nan(
        temp_home.path(),
        &server.base_url,
        &["set", "roomaji", "off"],
    ));
    assert_success(run_nan(
        temp_home.path(),
        &server.base_url,
        &["set", "furigana", "off"],
    ));
    let output = assert_success(run_nan(
        temp_home.path(),
        &server.base_url,
        &["add", "匿名的人真厉害啊"],
    ));
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(!stdout.contains("tokumei-san"));
    assert!(!stdout.contains("とくめいさん"));
    assert!(stdout.contains("匿名さん"));

    server.finish();
}

#[test]
fn add_renders_cleanly_with_compact_romaji_chunks() {
    let temp_home = TempDir::new().expect("temp home should exist");
    let server = MockServer::start(vec![MockResponse::json(success_body(json!({
        "japanese_sentence": "教室で写真を見ました。",
        "translated_sentence": "我在教室里看了照片。",
        "romaji_line": "kyoushitsu de shashin o mimashita",
        "furigana_line": "きょうしつ で しゃしん を みました",
        "tokens": [
            token_json("教室", Some("教室"), Some("きょうしつ"), Some("kyoushitsu"), "教室", "地点名词", ["教室", "きょうしつ"]),
            token_json("で", Some("で"), Some("で"), Some("de"), "在", "地点助词", ["で"]),
            token_json("写真", Some("写真"), Some("しゃしん"), Some("shashin"), "照片", "名词", ["写真", "しゃしん"]),
            token_json("を", Some("を"), Some("を"), Some("o"), "宾语标记", "宾语助词", ["を"]),
            token_json("見ました", Some("見る"), Some("みました"), Some("mimashita"), "看了", "过去礼貌形动词", ["見ました", "見る"]),
            token_json("。", Some("。"), None, None, "句号", "标点", ["。"])
        ]
    })))]);

    let output = assert_success(run_nan(
        temp_home.path(),
        &server.base_url,
        &["add", "我在教室里看了照片"],
    ));
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(stdout.contains("kyoushitsu"));
    assert!(stdout.contains("shashin"));
    assert!(stdout.contains("きょうしつ"));
    assert!(stdout.contains("しゃしん"));
    assert!(stdout.contains("教室"));
    assert!(stdout.contains("写真"));

    server.finish();
}

#[test]
fn add_renders_cleanly_with_only_romaji_hidden_for_compact_chunks() {
    let temp_home = TempDir::new().expect("temp home should exist");
    let server = MockServer::start(vec![MockResponse::json(success_body(json!({
        "japanese_sentence": "今日は小説を読みます。",
        "translated_sentence": "今天我要读小说。",
        "romaji_line": "kyou wa shousetsu o yomimasu",
        "furigana_line": "きょう は しょうせつ を よみます",
        "tokens": [
            token_json("今日", Some("今日"), Some("きょう"), Some("kyou"), "今天", "时间名词", ["今日", "きょう"]),
            token_json("は", Some("は"), Some("は"), Some("wa"), "主题标记", "主题助词", ["は"]),
            token_json("小説", Some("小説"), Some("しょうせつ"), Some("shousetsu"), "小说", "名词", ["小説", "しょうせつ"]),
            token_json("を", Some("を"), Some("を"), Some("o"), "宾语标记", "宾语助词", ["を"]),
            token_json("読みます", Some("読む"), Some("よみます"), Some("yomimasu"), "读", "礼貌形动词", ["読みます", "読む"]),
            token_json("。", Some("。"), None, None, "句号", "标点", ["。"])
        ]
    })))]);

    assert_success(run_nan(
        temp_home.path(),
        &server.base_url,
        &["set", "roomaji", "off"],
    ));
    let output = assert_success(run_nan(
        temp_home.path(),
        &server.base_url,
        &["add", "今天我要读小说"],
    ));
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(!stdout.contains("kyou"));
    assert!(!stdout.contains("shousetsu"));
    assert!(stdout.contains("きょう"));
    assert!(stdout.contains("しょうせつ"));
    assert!(stdout.contains("小説"));

    server.finish();
}

#[test]
fn add_renders_cleanly_with_only_furigana_hidden_for_compact_chunks() {
    let temp_home = TempDir::new().expect("temp home should exist");
    let server = MockServer::start(vec![MockResponse::json(success_body(json!({
        "japanese_sentence": "詩集を買いました。",
        "translated_sentence": "我买了诗集。",
        "romaji_line": "shishuu o kaimashita",
        "furigana_line": "ししゅう を かいました",
        "tokens": [
            token_json("詩集", Some("詩集"), Some("ししゅう"), Some("shishuu"), "诗集", "名词", ["詩集", "ししゅう"]),
            token_json("を", Some("を"), Some("を"), Some("o"), "宾语标记", "宾语助词", ["を"]),
            token_json("買いました", Some("買う"), Some("かいました"), Some("kaimashita"), "买了", "过去礼貌形动词", ["買いました", "買う"]),
            token_json("。", Some("。"), None, None, "句号", "标点", ["。"])
        ]
    })))]);

    assert_success(run_nan(
        temp_home.path(),
        &server.base_url,
        &["set", "furigana", "off"],
    ));
    let output = assert_success(run_nan(
        temp_home.path(),
        &server.base_url,
        &["add", "我买了诗集"],
    ));
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(stdout.contains("shishuu"));
    assert!(!stdout.contains("ししゅう"));
    assert!(stdout.contains("詩集"));

    server.finish();
}

#[test]
fn add_retries_transient_failures_before_succeeding() {
    let temp_home = TempDir::new().expect("temp home should exist");
    let server = MockServer::start(vec![
        MockResponse::status(503, "retry-once"),
        MockResponse::json(success_body(seed_add_payload())),
    ]);

    let output = assert_success(run_nan(
        temp_home.path(),
        &server.base_url,
        &["add", "我今天喝咖啡"],
    ));
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(stdout.contains("我今天喝咖啡。"));

    let database = load_database(temp_home.path());
    assert_eq!(database.sentences.len(), 1);

    let requests = server.finish();
    assert_eq!(requests.len(), 2);
}

#[test]
fn new_filters_highly_similar_sentences_by_word_overlap() {
    let temp_home = TempDir::new().expect("temp home should exist");
    let server = MockServer::start(vec![
        MockResponse::json(success_body(seed_add_payload())),
        MockResponse::json(success_body(json!({
            "sentences": [
                seed_add_payload(),
                json!({
                    "japanese_sentence": "今日は私がコーヒーを飲みます。",
                    "translated_sentence": "今天我来喝咖啡。",
                    "romaji_line": "kyou wa watashi ga koohii o nomimasu.",
                    "furigana_line": "今日[きょう]は私[わたし]がコーヒーを飲[の]みます。",
                    "tokens": [
                        token_json("今日", Some("今日"), Some("きょう"), Some("kyou"), "今天", "时间名词", ["今日", "きょう"]),
                        token_json("は", Some("は"), Some("は"), Some("wa"), "主题标记", "主题助词", ["は"]),
                        token_json("私", Some("私"), Some("わたし"), Some("watashi"), "我", "第一人称代词", ["私", "わたし"]),
                        token_json("が", Some("が"), Some("が"), Some("ga"), "主语标记", "主语助词", ["が"]),
                        token_json("コーヒー", Some("コーヒー"), Some("コーヒー"), Some("koohii"), "咖啡", "外来语名词", ["コーヒー"]),
                        token_json("を", Some("を"), Some("を"), Some("o"), "宾语标记", "宾语助词", ["を"]),
                        token_json("飲みます", Some("飲む"), Some("のみます"), Some("nomimasu"), "喝", "礼貌形动词", ["飲みます", "飲む"]),
                        token_json("。", Some("。"), None, None, "句号", "句末标点", ["。"])
                    ]
                }),
                json!({
                    "japanese_sentence": "私は明日学校へ行きます。",
                    "translated_sentence": "我明天去学校。",
                    "romaji_line": "watashi wa ashita gakkou e ikimasu.",
                    "furigana_line": "私[わたし]は 明日[あした] 学校[がっこう]へ 行[い]きます。",
                    "tokens": [
                        token_json("私", Some("私"), Some("わたし"), Some("watashi"), "我", "第一人称代词", ["私", "わたし"]),
                        token_json("は", Some("は"), Some("は"), Some("wa"), "主题标记", "主题助词", ["は"]),
                        token_json("明日", Some("明日"), Some("あした"), Some("ashita"), "明天", "时间名词", ["明日", "あした"]),
                        token_json("学校", Some("学校"), Some("がっこう"), Some("gakkou"), "学校", "地点名词", ["学校", "がっこう"]),
                        token_json("へ", Some("へ"), Some("へ"), Some("e"), "向、往", "方向助词", ["へ"]),
                        token_json("行きます", Some("行く"), Some("いきます"), Some("ikimasu"), "去", "礼貌形动词", ["行きます", "行く"]),
                        token_json("。", Some("。"), None, None, "句号", "句末标点", ["。"])
                    ]
                }),
                json!({
                    "japanese_sentence": "今日は寒いです。",
                    "translated_sentence": "今天很冷。",
                    "romaji_line": "kyou wa samui desu.",
                    "furigana_line": "今日[きょう]は 寒[さむ]いです。",
                    "tokens": [
                        token_json("今日", Some("今日"), Some("きょう"), Some("kyou"), "今天", "时间名词", ["今日", "きょう"]),
                        token_json("は", Some("は"), Some("は"), Some("wa"), "主题标记", "主题助词", ["は"]),
                        token_json("寒い", Some("寒い"), Some("さむい"), Some("samui"), "冷", "形容词", ["寒い", "さむい"]),
                        token_json("です", Some("です"), Some("です"), Some("desu"), "是", "礼貌语尾", ["です"]),
                        token_json("。", Some("。"), None, None, "句号", "句末标点", ["。"])
                    ]
                })
            ]
        }))),
    ]);

    assert_success(run_nan(
        temp_home.path(),
        &server.base_url,
        &["add", "我今天喝咖啡"],
    ));
    let output = assert_success(run_nan(
        temp_home.path(),
        &server.base_url,
        &["new", "2", "daily"],
    ));
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");

    assert!(stdout.contains("我明天去学校。"));
    assert!(stdout.contains("今天很冷。"));
    assert!(!stdout.contains("今日は私がコーヒーを飲みます。"));

    let database = load_database(temp_home.path());
    let stored_sentences = database
        .sentences
        .iter()
        .map(|sentence| sentence.source_text.as_str())
        .collect::<Vec<_>>();
    assert_eq!(stored_sentences.len(), 3);
    assert!(stored_sentences.contains(&"私は今日コーヒーを飲みます。"));
    assert!(stored_sentences.contains(&"私は明日学校へ行きます。"));
    assert!(stored_sentences.contains(&"今日は寒いです。"));
    assert!(!stored_sentences.contains(&"今日は私がコーヒーを飲みます。"));

    let requests = server.finish();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1]["model"], TEST_MODEL);
}

#[test]
fn add_keeps_dictionary_meaning_stable_when_context_is_negative() {
    let temp_home = TempDir::new().expect("temp home should exist");
    let server = MockServer::start(vec![
        MockResponse::json(success_body(json!({
            "japanese_sentence": "私はコーヒーを飲みます。",
            "translated_sentence": "我喝咖啡。",
            "romaji_line": "watashi wa koohii o nomimasu.",
            "furigana_line": "私[わたし]は コーヒーを 飲[の]みます。",
            "tokens": [
                token_json("私", Some("私"), Some("わたし"), Some("watashi"), "我", "第一人称代词", ["私", "わたし"]),
                token_json("コーヒー", Some("コーヒー"), Some("コーヒー"), Some("koohii"), "咖啡", "外来语名词", ["コーヒー"]),
                token_json("を", Some("を"), Some("を"), Some("o"), "宾语标记", "宾语助词", ["を"]),
                token_json("飲みます", Some("飲む"), Some("のみます"), Some("nomimasu"), "喝", "礼貌形动词", ["飲みます", "飲む"]),
                token_json("。", Some("。"), None, None, "句号", "句末标点", ["。"])
            ]
        }))),
        MockResponse::json(success_body(json!({
            "japanese_sentence": "私はコーヒーを飲みません。",
            "translated_sentence": "我不喝咖啡。",
            "romaji_line": "watashi wa koohii o nomimasen.",
            "furigana_line": "私[わたし]は コーヒーを 飲[の]みません。",
            "tokens": [
                token_json("私", Some("私"), Some("わたし"), Some("watashi"), "我", "第一人称代词", ["私", "わたし"]),
                token_json("コーヒー", Some("コーヒー"), Some("コーヒー"), Some("koohii"), "咖啡", "外来语名词", ["コーヒー"]),
                token_json("を", Some("を"), Some("を"), Some("o"), "宾语标记", "宾语助词", ["を"]),
                token_json_with_dictionary(
                    "飲みません",
                    Some("飲む"),
                    Some("のみません"),
                    Some("nomimasen"),
                    "不喝",
                    "礼貌否定形动词",
                    "喝",
                    "动词原形，表示喝",
                    ["飲みません", "飲む"],
                ),
                token_json("。", Some("。"), None, None, "句号", "句末标点", ["。"])
            ]
        }))),
    ]);

    assert_success(run_nan(
        temp_home.path(),
        &server.base_url,
        &["add", "我喝咖啡"],
    ));
    assert_success(run_nan(
        temp_home.path(),
        &server.base_url,
        &["add", "我不喝咖啡"],
    ));

    let database = load_database(temp_home.path());
    let drink_word = database
        .words
        .iter()
        .find(|word| word.canonical_form == "飲む")
        .expect("drink word should exist");
    assert_eq!(drink_word.translation, "喝");
    assert_eq!(drink_word.analysis, "动词原形，表示喝");
    assert_eq!(database.sentences[1].tokens[3].gloss.as_deref(), Some("喝"));
    assert_eq!(
        database.sentences[1].tokens[3].context_gloss.as_deref(),
        Some("不喝")
    );

    let requests = server.finish();
    assert_eq!(requests.len(), 2);
}

#[test]
fn set_lan_rewrites_sentences_and_words_and_syncs_sentence_glosses() {
    let temp_home = TempDir::new().expect("temp home should exist");
    let server = MockServer::start(vec![
        MockResponse::json(success_body(json!({
            "japanese_sentence": "猫です。",
            "translated_sentence": "这是猫。",
            "romaji_line": "neko desu.",
            "furigana_line": "猫[ねこ]です。",
            "tokens": [
                token_json("猫", Some("猫"), Some("ねこ"), Some("neko"), "猫", "名词", ["猫", "ねこ"]),
                token_json("です", Some("です"), Some("です"), Some("desu"), "是", "礼貌语尾", ["です"]),
                token_json("。", Some("。"), None, None, "句号", "标点", ["。"])
            ]
        }))),
        MockResponse::json(success_body(json!({"translated_sentence": "It is a cat."}))),
        MockResponse::json(success_body(
            json!({"translation": "cat", "analysis": "a noun meaning cat"}),
        )),
        MockResponse::json(success_body(
            json!({"translation": "is", "analysis": "a polite sentence ending"}),
        )),
    ]);

    assert_success(run_nan(
        temp_home.path(),
        &server.base_url,
        &["add", "这是猫"],
    ));
    assert_success(run_nan(
        temp_home.path(),
        &server.base_url,
        &["set", "lan", "english"],
    ));

    let database = load_database(temp_home.path());
    assert_eq!(database.settings.lan.as_str(), "english");
    assert!(database.language_rewrite.is_none());
    assert!(
        database
            .sentences
            .iter()
            .all(|sentence| sentence.lan.as_str() == "english")
    );
    assert!(
        database
            .words
            .iter()
            .all(|word| word.lan.as_str() == "english")
    );
    assert_eq!(database.sentences[0].translated_text, "It is a cat.");
    assert_eq!(
        database.sentences[0].tokens[0].gloss.as_deref(),
        Some("cat")
    );
    assert_eq!(
        database.sentences[0].tokens[0].context_gloss.as_deref(),
        Some("cat")
    );
    assert_eq!(database.sentences[0].tokens[2].gloss, None);
    assert_eq!(database.sentences[0].tokens[2].context_gloss, None);
    assert_eq!(database.words[0].translation, "cat");

    let requests = server.finish();
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[0]["model"], TEST_MODEL);
    assert_eq!(requests[3]["model"], TEST_MODEL);
}

#[test]
fn noninteractive_language_mismatch_returns_recovery_error() {
    let temp_home = TempDir::new().expect("temp home should exist");
    fs::write(
        temp_home.path().join(CONFIG_FILE_NAME),
        json!({
            "schema_version": 1,
            "settings": {
                "ref_capacity": 10,
                "level": "n5.5",
                "base_url": "https://api.openai.com/v1",
                "api_key": null,
                "model": "gpt-4o-mini",
                "romaji_enabled": true,
                "furigana_enabled": true,
                "lan": "english"
            },
            "sentences": [
                {
                    "id": 1,
                    "lan": "chinese",
                    "source_text": "猫です。",
                    "translated_text": "这是猫。",
                    "style": null,
                    "created_at_unix_secs": 0,
                    "updated_at_unix_secs": 0,
                    "romaji_line": "neko desu.",
                    "furigana_line": "猫[ねこ]です。",
                    "tokens": [],
                    "word_ids": [1],
                    "rewrite_status": "done",
                    "rewrite_error": null
                }
            ],
            "words": [
                {
                    "id": 1,
                    "lan": "chinese",
                    "canonical_form": "猫",
                    "translation": "猫",
                    "analysis": "名词",
                    "variants": ["猫", "ねこ"],
                    "source_sentence_ids": [1],
                    "s_last_days": 0.018,
                    "t_last_unix_secs": 0,
                    "created_at_unix_secs": 0,
                    "updated_at_unix_secs": 0,
                    "rewrite_status": "done",
                    "rewrite_error": null
                }
            ],
            "language_rewrite": null,
            "next_sentence_id": 2,
            "next_word_id": 2
        })
        .to_string(),
    )
    .expect("mismatch config should write");

    let output = assert_failure(run_nan(temp_home.path(), "http://127.0.0.1:1", &["list"]));
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("stored data uses inconsistent languages"));
    assert!(stderr.contains("interactive terminal"));
}

#[test]
fn add_reports_error_after_retries_are_exhausted() {
    let temp_home = TempDir::new().expect("temp home should exist");
    let server = MockServer::start(vec![
        MockResponse::status(503, "retry-1"),
        MockResponse::status(503, "retry-2"),
        MockResponse::status(503, "retry-3"),
    ]);

    let output = assert_failure(run_nan(
        temp_home.path(),
        &server.base_url,
        &["add", "我今天喝咖啡"],
    ));
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("AI request failed with HTTP status 503"));
    assert!(stderr.contains("retry-3"));

    let database = load_database(temp_home.path());
    assert!(database.sentences.is_empty());
    assert!(database.words.is_empty());

    let requests = server.finish();
    assert_eq!(requests.len(), 3);
}

fn seed_add_payload() -> Value {
    json!({
        "japanese_sentence": "私は今日コーヒーを飲みます。",
        "translated_sentence": "我今天喝咖啡。",
        "romaji_line": "watashi wa kyou koohii o nomimasu.",
        "furigana_line": "私[わたし]は 今日[きょう] コーヒーを 飲[の]みます。",
        "tokens": [
            token_json("私", Some("私"), Some("わたし"), Some("watashi"), "我", "第一人称代词", ["私", "わたし"]),
            token_json("は", Some("は"), Some("は"), Some("wa"), "主题标记", "主题助词", ["は"]),
            token_json("今日", Some("今日"), Some("きょう"), Some("kyou"), "今天", "时间名词", ["今日", "きょう"]),
            token_json("コーヒー", Some("コーヒー"), Some("コーヒー"), Some("koohii"), "咖啡", "外来语名词", ["コーヒー"]),
            token_json("を", Some("を"), Some("を"), Some("o"), "宾语标记", "宾语助词", ["を"]),
            token_json("飲みます", Some("飲む"), Some("のみます"), Some("nomimasu"), "喝", "礼貌形动词", ["飲みます", "飲む"]),
            token_json("。", Some("。"), None, None, "句号", "句末标点", ["。"])
        ]
    })
}

fn token_json<const N: usize>(
    surface: &str,
    lemma: Option<&str>,
    reading: Option<&str>,
    romaji: Option<&str>,
    gloss: &str,
    analysis: &str,
    variants: [&str; N],
) -> Value {
    json!({
        "surface": surface,
        "reading": reading,
        "romaji": romaji,
        "lemma": lemma,
        "gloss": gloss,
        "analysis": analysis,
        "dictionary_gloss": gloss,
        "dictionary_analysis": analysis,
        "variants": variants.into_iter().collect::<Vec<_>>(),
        "spans": [
            {
                "text": surface,
                "reading": reading
            }
        ]
    })
}

fn write_legacy_v1_database(home: &Path) {
    fs::write(
        home.join(CONFIG_FILE_NAME),
        legacy_v1_database_json().to_string(),
    )
    .expect("legacy config should write");
}

fn legacy_v1_database_json() -> Value {
    json!({
        "schema_version": 1,
        "settings": {
            "ref_capacity": 10,
            "level": "n5.5",
            "base_url": "https://api.openai.com/v1",
            "api_key": null,
            "model": "gpt-4o-mini",
            "romaji_enabled": true,
            "furigana_enabled": true,
            "lan": "chinese"
        },
        "sentences": [
            {
                "id": 1,
                "lan": "chinese",
                "source_text": "私はコーヒーを飲みません。",
                "translated_text": "我不喝咖啡。",
                "style": null,
                "created_at_unix_secs": 0,
                "updated_at_unix_secs": 0,
                "romaji_line": "watashi wa koohii o nomimasen.",
                "furigana_line": "私[わたし]は コーヒーを 飲[の]みません。",
                "tokens": [
                    {
                        "surface": "飲みません",
                        "reading": "のみません",
                        "romaji": "nomimasen",
                        "lemma": "飲む",
                        "gloss": "不喝",
                        "variants": ["飲みません", "飲む"],
                        "spans": [
                            {
                                "text": "飲みません",
                                "reading": "のみません"
                            }
                        ]
                    }
                ],
                "word_ids": [1],
                "rewrite_status": "done",
                "rewrite_error": null
            }
        ],
        "words": [
            {
                "id": 1,
                "lan": "chinese",
                "canonical_form": "飲む",
                "translation": "喝",
                "analysis": "动词原形，表示喝",
                "variants": ["飲みません", "飲む"],
                "source_sentence_ids": [1],
                "s_last_days": 0.018,
                "t_last_unix_secs": 0,
                "created_at_unix_secs": 0,
                "updated_at_unix_secs": 0,
                "rewrite_status": "done",
                "rewrite_error": null
            }
        ],
        "language_rewrite": null,
        "next_sentence_id": 2,
        "next_word_id": 2
    })
}

#[allow(clippy::too_many_arguments)]
fn token_json_with_dictionary<const N: usize>(
    surface: &str,
    lemma: Option<&str>,
    reading: Option<&str>,
    romaji: Option<&str>,
    gloss: &str,
    analysis: &str,
    dictionary_gloss: &str,
    dictionary_analysis: &str,
    variants: [&str; N],
) -> Value {
    json!({
        "surface": surface,
        "reading": reading,
        "romaji": romaji,
        "lemma": lemma,
        "gloss": gloss,
        "analysis": analysis,
        "dictionary_gloss": dictionary_gloss,
        "dictionary_analysis": dictionary_analysis,
        "variants": variants.into_iter().collect::<Vec<_>>(),
        "spans": [
            {
                "text": surface,
                "reading": reading
            }
        ]
    })
}

fn success_body(content: Value) -> Value {
    json!({
        "choices": [
            {
                "message": {
                    "content": content.to_string()
                }
            }
        ]
    })
}

fn run_nan(home: &Path, base_url: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nan"))
        .args(args)
        .env("HOME", home)
        .env("NAN_OPENAI_BASE_URL", base_url)
        .env("NAN_OPENAI_API_KEY", TEST_API_KEY)
        .env("NAN_OPENAI_MODEL", TEST_MODEL)
        .output()
        .expect("nan command should run")
}

fn assert_success(output: Output) -> Output {
    if !output.status.success() {
        panic!(
            "command failed\nstatus: {:?}\nstdout: {}\nstderr: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
    output
}

fn assert_failure(output: Output) -> Output {
    if output.status.success() {
        panic!(
            "command unexpectedly succeeded\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
    output
}

fn load_database(home: &Path) -> Database {
    let path = home.join(CONFIG_FILE_NAME);
    let content = fs::read_to_string(path).expect("config file should exist");
    serde_json::from_str(&content).expect("config should parse")
}

struct MockServer {
    base_url: String,
    requests: Arc<Mutex<Vec<Value>>>,
    handle: Option<JoinHandle<()>>,
}

impl MockServer {
    fn start(responses: Vec<MockResponse>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
        let address = listener.local_addr().expect("listener should have address");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let thread_requests = Arc::clone(&requests);
        let responses = Arc::new(Mutex::new(VecDeque::from(responses)));
        let thread_responses = Arc::clone(&responses);

        let handle = thread::spawn(move || {
            while let Some(response) = thread_responses
                .lock()
                .expect("lock should work")
                .pop_front()
            {
                let (mut stream, _) = listener.accept().expect("request should arrive");
                let request_body = read_http_request(&mut stream);
                let request_json: Value =
                    serde_json::from_str(&request_body).expect("request body should be json");
                thread_requests
                    .lock()
                    .expect("lock should work")
                    .push(request_json);
                write_http_response(&mut stream, &response);
            }
        });

        Self {
            base_url: format!("http://{address}"),
            requests,
            handle: Some(handle),
        }
    }

    fn finish(mut self) -> Vec<Value> {
        if let Some(handle) = self.handle.take() {
            handle.join().expect("mock server should finish cleanly");
        }
        Arc::try_unwrap(self.requests)
            .expect("no other request owners should remain")
            .into_inner()
            .expect("request lock should be available")
    }
}

struct MockResponse {
    status: u16,
    body: String,
    content_type: &'static str,
}

impl MockResponse {
    fn json(body: Value) -> Self {
        Self {
            status: 200,
            body: body.to_string(),
            content_type: "application/json",
        }
    }

    fn status(status: u16, body: &str) -> Self {
        Self {
            status,
            body: body.to_string(),
            content_type: "text/plain",
        }
    }
}

fn read_http_request(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout should set");
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 2048];
    let mut expected_length = None;
    let mut body_start = None;

    loop {
        let read = stream.read(&mut chunk).expect("request should be readable");
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);

        if body_start.is_none()
            && let Some(index) = buffer.windows(4).position(|window| window == b"\r\n\r\n")
        {
            body_start = Some(index + 4);
            expected_length = Some(parse_content_length(&buffer[..index + 4]));
        }

        if let (Some(start), Some(length)) = (body_start, expected_length)
            && buffer.len() >= start + length
        {
            let body = &buffer[start..start + length];
            return String::from_utf8(body.to_vec()).expect("body should be utf8");
        }
    }

    panic!("request body was incomplete");
}

fn parse_content_length(headers: &[u8]) -> usize {
    let headers = String::from_utf8(headers.to_vec()).expect("headers should be utf8");
    headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case("content-length") {
                Some(
                    value
                        .trim()
                        .parse::<usize>()
                        .expect("content length should parse"),
                )
            } else {
                None
            }
        })
        .unwrap_or(0)
}

fn write_http_response(stream: &mut TcpStream, response: &MockResponse) {
    let status_text = match response.status {
        200 => "OK",
        400 => "Bad Request",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "OK",
    };

    write!(
        stream,
        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nContent-Type: {}\r\nConnection: close\r\n\r\n{}",
        response.status,
        status_text,
        response.body.len(),
        response.content_type,
        response.body,
    )
    .expect("response should write");
    stream.flush().expect("response should flush");
}
