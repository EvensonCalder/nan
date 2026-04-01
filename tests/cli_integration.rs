use std::collections::VecDeque;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use nan::model::Database;
use nan::store::CONFIG_FILE_NAME;
use serde_json::{Value, json};
use tempfile::TempDir;

const TEST_API_KEY: &str = "integration-test-key";
const TEST_MODEL: &str = "integration-test-model";

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
                token_json("。", Some("。"), None, None, "句号", "句末标点", ["。"])
            ]
        }))),
        MockResponse::json(success_body(json!({"translated_sentence": "It is a cat."}))),
        MockResponse::json(success_body(
            json!({"translation": "cat", "analysis": "a noun meaning cat"}),
        )),
        MockResponse::json(success_body(
            json!({"translation": "is", "analysis": "a polite sentence ending"}),
        )),
        MockResponse::json(success_body(
            json!({"translation": "period", "analysis": "sentence-ending punctuation"}),
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
    assert_eq!(database.words[0].translation, "cat");

    let requests = server.finish();
    assert_eq!(requests.len(), 5);
    assert_eq!(requests[0]["model"], TEST_MODEL);
    assert_eq!(requests[4]["model"], TEST_MODEL);
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
