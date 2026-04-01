use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::NanError;
use crate::model::Settings;

const CHAT_COMPLETIONS_PATH: &str = "/chat/completions";

#[derive(Debug, Clone)]
pub struct AiClient {
    base_url: String,
    api_key: String,
    model: String,
}

impl AiClient {
    pub fn from_settings(settings: &Settings) -> Result<Self, NanError> {
        let api_key = settings.api_key.clone().ok_or_else(|| {
            NanError::message("API key is not configured. Run `nan set api-key <key>` first.")
        })?;

        Ok(Self {
            base_url: settings.base_url.clone(),
            api_key,
            model: settings.model.clone(),
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

        let response = ureq::post(&endpoint)
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .set("Content-Type", "application/json")
            .send_json(payload);

        let response = match response {
            Ok(response) => response,
            Err(ureq::Error::Status(status, response)) => {
                let body = response
                    .into_string()
                    .unwrap_or_else(|_| "<unable to read response body>".to_string());
                return Err(NanError::message(format!(
                    "AI request failed with HTTP status {status}: {body}"
                )));
            }
            Err(error) => {
                return Err(NanError::message(format!("AI request failed: {error}")));
            }
        };

        let completion: ChatCompletionResponse = response.into_json().map_err(|error| {
            NanError::message(format!("failed to decode AI response as JSON: {error}"))
        })?;

        let content = completion.first_text_content()?;
        serde_json::from_str(&content).map_err(|error| {
            NanError::message(format!(
                "failed to parse AI JSON payload: {error}; payload: {content}"
            ))
        })
    }
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
    use super::{ChatCompletionResponse, MessageContent};

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
}
