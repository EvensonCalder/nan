use crate::model::{NativeLanguage, ProficiencyLevel};

pub fn add_system_prompt() -> &'static str {
    "You are a precise Japanese learning assistant. You must return only valid JSON that matches the requested schema. Do not include markdown fences. Do not include any explanatory text outside the JSON object. Keep the Japanese natural, idiomatic, and appropriate for the requested proficiency level."
}

pub fn build_add_user_prompt(
    input_sentence: &str,
    style: Option<&str>,
    level: ProficiencyLevel,
    native_language: NativeLanguage,
) -> String {
    let mut prompt = String::new();
    prompt.push_str("Task:\n");
    prompt.push_str("1. Translate the input sentence into natural Japanese.\n");
    prompt.push_str("2. Preserve the original meaning.\n");
    prompt.push_str("3. Match the requested proficiency level.\n");
    prompt.push_str(
        "4. If a style is provided, gently reflect that style without becoming unnatural.\n",
    );
    prompt.push_str("5. Provide one line of romaji for the whole sentence.\n");
    prompt.push_str("6. Provide one line of furigana for the whole sentence.\n");
    prompt.push_str("7. Split the Japanese sentence into meaningful tokens.\n");
    prompt.push_str(
        "8. For each token, provide a gloss and a short learning-oriented analysis in the native language.\n",
    );
    prompt.push_str(
        "9. For each token, provide enough inflectional variants to help deduplicate common forms of the same word.\n",
    );
    prompt
        .push_str("10. For mixed kanji-kana words, provide span-level furigana when possible.\n\n");

    prompt.push_str(&format!("Input sentence: {input_sentence}\n"));
    prompt.push_str(&format!("Target proficiency level: {}\n", level.as_str()));
    prompt.push_str(&format!(
        "Native language for translation and token analysis: {}\n",
        native_language.as_str()
    ));

    if let Some(style) = style {
        prompt.push_str(&format!("Requested style: {style}\n"));
    } else {
        prompt.push_str("Requested style: none\n");
    }

    prompt.push_str("\nReturn exactly one JSON object with this schema:\n");
    prompt.push_str(
        r#"{
  "japanese_sentence": "string",
  "translated_sentence": "string in the requested native language",
  "romaji_line": "string",
  "furigana_line": "string",
  "tokens": [
    {
      "surface": "string",
      "reading": "string or null",
      "romaji": "string or null",
      "lemma": "dictionary form or null",
      "gloss": "short translation in the requested native language",
      "analysis": "short learner-friendly explanation in the requested native language",
      "variants": ["string", "..."],
      "spans": [
        {
          "text": "string",
          "reading": "string or null"
        }
      ]
    }
  ]
}"#,
    );
    prompt.push_str("\n\nRules:\n");
    prompt.push_str("- The JSON must be valid.\n");
    prompt.push_str("- `tokens` must not be empty.\n");
    prompt.push_str("- Every token must have at least one variant, and the surface form must be included in variants.\n");
    prompt.push_str("- Keep punctuation as its own token when reasonable.\n");
    prompt.push_str("- `analysis` should be concise and useful for a learner.\n");
    prompt.push_str("- Use only the requested native language for `translated_sentence`, `gloss`, and `analysis`.\n");
    prompt
}

pub fn build_new_user_prompt(
    count: usize,
    style: Option<&str>,
    level: ProficiencyLevel,
    native_language: NativeLanguage,
    reference_words: &[String],
    reference_sentences: &[String],
) -> String {
    let mut prompt = String::new();
    prompt.push_str("Task:\n");
    prompt.push_str("1. Generate natural new Japanese sentences for study.\n");
    prompt.push_str("2. Match the requested proficiency level.\n");
    prompt.push_str("3. Reuse the reference words when reasonable, especially the weaker ones.\n");
    prompt.push_str(
        "4. Do not generate sentences that are too similar to the reference sentences.\n",
    );
    prompt.push_str("5. Return exactly the requested number of candidates.\n");
    prompt.push_str("6. For every generated sentence, provide the same token analysis schema as in the add flow.\n\n");

    prompt.push_str(&format!("Candidate count: {}\n", count * 2));
    prompt.push_str(&format!("Target proficiency level: {}\n", level.as_str()));
    prompt.push_str(&format!(
        "Native language for translation and token analysis: {}\n",
        native_language.as_str()
    ));

    if let Some(style) = style {
        prompt.push_str(&format!("Requested style: {style}\n"));
    } else {
        prompt.push_str("Requested style: none\n");
    }

    if reference_words.is_empty() {
        prompt.push_str("Reference words: none\n");
    } else {
        prompt.push_str("Reference words:\n");
        for word in reference_words {
            prompt.push_str("- ");
            prompt.push_str(word);
            prompt.push('\n');
        }
    }

    if reference_sentences.is_empty() {
        prompt.push_str("Reference sentences to avoid similarity with: none\n");
    } else {
        prompt.push_str("Reference sentences to avoid similarity with:\n");
        for sentence in reference_sentences {
            prompt.push_str("- ");
            prompt.push_str(sentence);
            prompt.push('\n');
        }
    }

    prompt.push_str("\nReturn exactly one JSON object with this schema:\n");
    prompt.push_str(
        r#"{
  "sentences": [
    {
      "japanese_sentence": "string",
      "translated_sentence": "string in the requested native language",
      "romaji_line": "string",
      "furigana_line": "string",
      "tokens": [
        {
          "surface": "string",
          "reading": "string or null",
          "romaji": "string or null",
          "lemma": "dictionary form or null",
          "gloss": "short translation in the requested native language",
          "analysis": "short learner-friendly explanation in the requested native language",
          "variants": ["string", "..."],
          "spans": [
            {
              "text": "string",
              "reading": "string or null"
            }
          ]
        }
      ]
    }
  ]
}"#,
    );
    prompt.push_str("\n\nRules:\n");
    prompt.push_str("- The JSON must be valid.\n");
    prompt.push_str("- Every sentence must be distinct.\n");
    prompt.push_str("- Every sentence must contain at least one token.\n");
    prompt.push_str(
        "- Use only the requested native language for translations and token analyses.\n",
    );
    prompt.push_str("- Prefer sentence patterns that help review weak words naturally.\n");
    prompt
}

pub fn rewrite_system_prompt() -> &'static str {
    "You rewrite learner-facing translations and analyses. Return only valid JSON. Do not change the underlying Japanese content."
}

pub fn build_sentence_rewrite_prompt(
    japanese_sentence: &str,
    current_translation: &str,
    native_language: NativeLanguage,
) -> String {
    format!(
        "Rewrite the translation of this Japanese sentence into {language}.\nJapanese sentence: {japanese_sentence}\nCurrent translation: {current_translation}\n\nReturn exactly one JSON object:\n{{\n  \"translated_sentence\": \"string in {language}\"\n}}",
        language = native_language.as_str(),
    )
}

pub fn build_word_rewrite_prompt(
    canonical_form: &str,
    current_translation: &str,
    current_analysis: &str,
    native_language: NativeLanguage,
) -> String {
    format!(
        "Rewrite the translation and short learner-facing analysis of this Japanese word into {language}.\nWord: {canonical_form}\nCurrent translation: {current_translation}\nCurrent analysis: {current_analysis}\n\nReturn exactly one JSON object:\n{{\n  \"translation\": \"short translation in {language}\",\n  \"analysis\": \"short learner-facing explanation in {language}\"\n}}",
        language = native_language.as_str(),
    )
}

#[cfg(test)]
mod tests {
    use crate::model::{NativeLanguage, ProficiencyLevel};

    use super::{
        build_add_user_prompt, build_new_user_prompt, build_sentence_rewrite_prompt,
        build_word_rewrite_prompt,
    };

    #[test]
    fn add_prompt_mentions_style_when_present() {
        let prompt = build_add_user_prompt(
            "今晚的月色真美",
            Some("Natsume Soseki"),
            ProficiencyLevel::N4,
            NativeLanguage::Chinese,
        );
        assert!(prompt.contains("Requested style: Natsume Soseki"));
        assert!(prompt.contains("Target proficiency level: n4"));
        assert!(prompt.contains("Native language for translation and token analysis: chinese"));
    }

    #[test]
    fn new_prompt_mentions_candidate_count_and_references() {
        let prompt = build_new_user_prompt(
            2,
            Some("daily"),
            ProficiencyLevel::N5,
            NativeLanguage::English,
            &["食べる (eat)".to_string()],
            &["今朝はパンを食べました。".to_string()],
        );
        assert!(prompt.contains("Candidate count: 4"));
        assert!(prompt.contains("Reference words:"));
        assert!(prompt.contains("Reference sentences to avoid similarity with:"));
        assert!(prompt.contains("Requested style: daily"));
    }

    #[test]
    fn rewrite_prompts_include_target_language() {
        let sentence_prompt = build_sentence_rewrite_prompt(
            "今夜は月がきれいですね。",
            "Tonight the moon is beautiful.",
            NativeLanguage::Chinese,
        );
        let word_prompt = build_word_rewrite_prompt(
            "食べる",
            "eat",
            "dictionary form of to eat",
            NativeLanguage::English,
        );
        assert!(sentence_prompt.contains("chinese"));
        assert!(word_prompt.contains("english"));
    }
}
