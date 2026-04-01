use crate::model::{SentenceRecord, Settings};

pub fn render_sentence(sentence: &SentenceRecord, settings: &Settings) -> String {
    let mut lines = Vec::new();
    lines.push(sentence.translated_text.clone());

    if settings.romaji_enabled && !sentence.romaji_line.trim().is_empty() {
        lines.push(sentence.romaji_line.clone());
    }

    if settings.furigana_enabled && !sentence.furigana_line.trim().is_empty() {
        lines.push(sentence.furigana_line.clone());
    }

    lines.push(sentence.source_text.clone());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use crate::model::{NativeLanguage, RewriteStatus, SentenceRecord, Settings};

    use super::render_sentence;

    fn sample_sentence() -> SentenceRecord {
        SentenceRecord {
            id: 1,
            lan: NativeLanguage::Chinese,
            source_text: "今夜は月がきれいですね。".to_string(),
            translated_text: "今晚的月色真美。".to_string(),
            style: None,
            created_at_unix_secs: 0,
            updated_at_unix_secs: 0,
            romaji_line: "kon'ya wa tsuki ga kirei desu ne.".to_string(),
            furigana_line: "こんや は つき が きれい です ね。".to_string(),
            tokens: Vec::new(),
            word_ids: Vec::new(),
            rewrite_status: RewriteStatus::Done,
            rewrite_error: None,
        }
    }

    #[test]
    fn render_obeys_visibility_settings() {
        let sentence = sample_sentence();
        let settings = Settings::default();
        let rendered = render_sentence(&sentence, &settings);
        assert!(rendered.contains("今晚的月色真美。"));
        assert!(rendered.contains("kon'ya wa tsuki ga kirei desu ne."));
        assert!(rendered.contains("こんや は つき が きれい です ね。"));
        assert!(rendered.contains("今夜は月がきれいですね。"));
    }
}
