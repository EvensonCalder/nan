use std::env;
use std::thread;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::NanError;
use crate::model::Settings;

const CHAT_COMPLETIONS_PATH: &str = "/chat/completions";
const ENV_BASE_URL: &str = "NAN_OPENAI_BASE_URL";
const ENV_API_KEY: &str = "NAN_OPENAI_API_KEY";
const ENV_MODEL: &str = "NAN_OPENAI_MODEL";
const MAX_RETRY_ATTEMPTS: usize = 3;
const INITIAL_RETRY_DELAY_MILLIS: u64 = 150;

#[derive(Debug, Clone)]
pub struct AiClient {
    base_url: String,
    api_key: String,
    model: String,
}

impl AiClient {
    pub fn from_settings(settings: &Settings) -> Result<Self, NanError> {
        let base_url = preferred_string(env::var(ENV_BASE_URL).ok(), Some(&settings.base_url))
            .ok_or_else(|| {
                NanError::message(
                    "base URL is not configured. Set NAN_OPENAI_BASE_URL or run `nan set base-url <url>`.",
                )
            })?;
        let api_key = preferred_string(env::var(ENV_API_KEY).ok(), settings.api_key.as_deref())
            .ok_or_else(|| {
                NanError::message(
                    "API key is not configured. Set NAN_OPENAI_API_KEY or run `nan set api-key <key>` first.",
                )
            })?;
        let model =
            preferred_string(env::var(ENV_MODEL).ok(), Some(&settings.model)).ok_or_else(|| {
                NanError::message(
                    "model is not configured. Set NAN_OPENAI_MODEL or run `nan set model <name>`.",
                )
            })?;

        Ok(Self {
            base_url,
            api_key,
            model,
        })
    }

    pub fn chat_json<T: DeserializeOwned>(
        &self,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Result<T, NanError> {
        let endpoint = format!(
            "{}{}",
            self.base_url.trim_end_matches('/'),
            CHAT_COMPLETIONS_PATH
        );
        let payload = json!({
            "model": self.model,
            "temperature": 0.2,
            "response_format": { "type": "json_object" },
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_prompt }
            ]
        });

        let response = self.send_with_retry(&endpoint, payload)?;

        let completion: ChatCompletionResponse = response.into_json().map_err(|error| {
            NanError::message(format!("failed to decode AI response as JSON: {error}"))
        })?;

        let content = completion.first_text_content()?;
        serde_json::from_str(content.trim()).map_err(|error| {
            NanError::message(format!(
                "failed to parse AI JSON payload from assistant content: {error}; payload: {content}"
            ))
        })
    }

    fn send_with_retry(
        &self,
        endpoint: &str,
        payload: serde_json::Value,
    ) -> Result<ureq::Response, NanError> {
        let mut delay = Duration::from_millis(INITIAL_RETRY_DELAY_MILLIS);

        for attempt in 1..=MAX_RETRY_ATTEMPTS {
            match self.send_request(endpoint, payload.clone()) {
                Ok(response) => return Ok(response),
                Err(RequestError::Permanent(message)) => return Err(NanError::message(message)),
                Err(RequestError::Transient(message)) => {
                    if attempt == MAX_RETRY_ATTEMPTS {
                        return Err(NanError::message(message));
                    }
                    thread::sleep(delay);
                    delay = delay.saturating_mul(2);
                }
            }
        }

        Err(NanError::message(
            "AI request retry loop exited unexpectedly",
        ))
    }

    fn send_request(
        &self,
        endpoint: &str,
        payload: serde_json::Value,
    ) -> Result<ureq::Response, RequestError> {
        match ureq::post(endpoint)
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .set("Content-Type", "application/json")
            .send_json(payload)
        {
            Ok(response) => Ok(response),
            Err(ureq::Error::Status(status, response)) => {
                let body = response
                    .into_string()
                    .unwrap_or_else(|_| "<unable to read response body>".to_string());
                let message = format!("AI request failed with HTTP status {status}: {body}");
                if is_transient_status(status) {
                    Err(RequestError::Transient(message))
                } else {
                    Err(RequestError::Permanent(message))
                }
            }
            Err(error) => Err(RequestError::Transient(format!(
                "AI request failed: {error}"
            ))),
        }
    }
}

#[derive(Debug)]
enum RequestError {
    Transient(String),
    Permanent(String),
}

fn is_transient_status(status: u16) -> bool {
    matches!(status, 408 | 409 | 425 | 429) || (500..600).contains(&status)
}

fn preferred_string(env_value: Option<String>, config_value: Option<&str>) -> Option<String> {
    env_value
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            config_value
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

impl ChatCompletionResponse {
    fn first_text_content(self) -> Result<String, NanError> {
        let Some(choice) = self.choices.into_iter().next() else {
            return Err(NanError::message("AI response did not contain any choices"));
        };

        match choice.message.content {
            MessageContent::Text(text) => Ok(text),
            MessageContent::Parts(parts) => {
                let mut combined = String::new();
                for part in parts {
                    if let ContentPart::Text { text } = part {
                        combined.push_str(&text);
                    }
                }

                if combined.trim().is_empty() {
                    Err(NanError::message(
                        "AI response contained no textual message content",
                    ))
                } else {
                    Ok(combined)
                }
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    content: MessageContent,
    #[allow(dead_code)]
    reasoning_content: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AddAiResponse {
    pub japanese_sentence: String,
    pub translated_sentence: String,
    pub romaji_line: String,
    pub furigana_line: String,
    pub tokens: Vec<AddAiToken>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NewAiResponse {
    pub sentences: Vec<AddAiResponse>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SentenceRewriteAiResponse {
    pub translated_sentence: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WordRewriteAiResponse {
    pub translation: String,
    pub analysis: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AddAiToken {
    pub surface: String,
    pub reading: Option<String>,
    pub romaji: Option<String>,
    pub lemma: Option<String>,
    pub gloss: String,
    pub analysis: String,
    pub variants: Vec<String>,
    pub spans: Vec<AddAiSpan>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AddAiSpan {
    pub text: String,
    pub reading: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use serde::Deserialize;

    use super::{ChatCompletionResponse, MessageContent, preferred_string};

    #[test]
    fn extracts_string_content_from_response() {
        let response: ChatCompletionResponse = serde_json::from_str(
            r#"{
                "choices": [
                    {
                        "message": {
                            "content": "{\"ok\":true}"
                        }
                    }
                ]
            }"#,
        )
        .expect("response should parse");

        let content = response.first_text_content().expect("content should exist");
        assert_eq!(content, r#"{"ok":true}"#);
    }

    #[test]
    fn extracts_text_parts_from_response() {
        let response: ChatCompletionResponse = serde_json::from_str(
            r#"{
                "choices": [
                    {
                        "message": {
                            "content": [
                                {"type": "text", "text": "{"},
                                {"type": "text", "text": "\"ok\":true}"}
                            ]
                        }
                    }
                ]
            }"#,
        )
        .expect("response should parse");

        let content = response.first_text_content().expect("content should exist");
        assert_eq!(content, r#"{"ok":true}"#);
    }

    #[test]
    fn message_content_untagged_accepts_text() {
        let content: MessageContent =
            serde_json::from_str("\"hello\"").expect("plain text content should parse");
        match content {
            MessageContent::Text(text) => assert_eq!(text, "hello"),
            MessageContent::Parts(_) => panic!("expected text content"),
        }
    }

    #[test]
    fn preferred_string_uses_environment_value_first() {
        let resolved = preferred_string(
            Some("https://env.example/v1".to_string()),
            Some("https://config.example/v1"),
        );
        assert_eq!(resolved.as_deref(), Some("https://env.example/v1"));
    }

    #[test]
    fn preferred_string_falls_back_to_config_when_env_is_blank() {
        let resolved = preferred_string(Some("   ".to_string()), Some("gpt-4o-mini"));
        assert_eq!(resolved.as_deref(), Some("gpt-4o-mini"));
    }

    #[test]
    fn ignores_reasoning_content_field() {
        let response: ChatCompletionResponse = serde_json::from_str(
            r#"{
                "choices": [
                    {
                        "message": {
                            "reasoning_content": "hidden chain of thought",
                            "content": "{\"ok\":true}"
                        }
                    }
                ]
            }"#,
        )
        .expect("response should parse");

        let content = response.first_text_content().expect("content should exist");
        assert_eq!(content, r#"{"ok":true}"#);
    }

    #[test]
    fn transient_status_is_retryable() {
        assert!(super::is_transient_status(429));
        assert!(super::is_transient_status(503));
        assert!(!super::is_transient_status(400));
    }

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct RetryPayload {
        ok: bool,
    }

    #[test]
    fn chat_json_retries_transient_http_failures() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
        let address = listener.local_addr().expect("listener should have address");

        let server = thread::spawn(move || {
            for attempt in 0..2 {
                let (mut stream, _) = listener.accept().expect("request should arrive");
                let mut buffer = [0_u8; 4096];
                let _ = stream
                    .read(&mut buffer)
                    .expect("request should be readable");

                if attempt == 0 {
                    write!(
                        stream,
                        "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 5\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nretry"
                    )
                    .expect("response should write");
                } else {
                    let body = r#"{"choices":[{"message":{"content":"{\"ok\":true}"}}]}"#;
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .expect("response should write");
                }
                stream.flush().expect("response should flush");
            }
        });

        let client = super::AiClient {
            base_url: format!("http://{address}"),
            api_key: "test-key".to_string(),
            model: "test-model".to_string(),
        };

        let payload: RetryPayload = client
            .chat_json("system", "user")
            .expect("request should succeed after retry");
        assert_eq!(payload, RetryPayload { ok: true });
        server.join().expect("server thread should finish");
    }
}
