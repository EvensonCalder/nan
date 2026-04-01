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

#[cfg(test)]
mod tests {
    use crate::model::{NativeLanguage, ProficiencyLevel};

    use super::build_add_user_prompt;

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
}
