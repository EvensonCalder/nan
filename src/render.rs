use crate::model::{SentenceRecord, SentenceToken, Settings};

#[derive(Debug, Clone)]
struct GroupCell {
    surface: String,
    romaji: Option<String>,
    furigana: Option<String>,
    width: usize,
}

pub fn render_sentence(sentence: &SentenceRecord, settings: &Settings) -> String {
    let mut lines = vec![sentence.translated_text.clone()];

    if sentence.tokens.is_empty() {
        if settings.romaji_enabled && !sentence.romaji_line.trim().is_empty() {
            lines.push(sentence.romaji_line.clone());
        }
        if settings.furigana_enabled && !sentence.furigana_line.trim().is_empty() {
            lines.push(sentence.furigana_line.clone());
        }
        lines.push(sentence.source_text.clone());
        return lines.join("\n");
    }

    let groups = build_groups(sentence);
    if settings.romaji_enabled {
        lines.push(render_row(&groups, |group| group.romaji.as_deref()));
    }
    if settings.furigana_enabled {
        lines.push(render_row(&groups, |group| group.furigana.as_deref()));
    }
    lines.push(render_row(&groups, |group| Some(group.surface.as_str())));
    lines.join("\n")
}

fn build_groups(sentence: &SentenceRecord) -> Vec<GroupCell> {
    sentence.tokens.iter().map(build_group).collect()
}

fn build_group(token: &SentenceToken) -> GroupCell {
    let surface = token.surface.clone();
    let romaji = token
        .romaji
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned);
    let furigana = resolve_furigana(token);
    let width = display_width(&surface)
        .max(romaji.as_deref().map(display_width).unwrap_or(0))
        .max(furigana.as_deref().map(display_width).unwrap_or(0));

    GroupCell {
        surface,
        romaji,
        furigana,
        width,
    }
}

fn resolve_furigana(token: &SentenceToken) -> Option<String> {
    token
        .reading
        .as_deref()
        .map(str::trim)
        .filter(|reading| !reading.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            let joined = token
                .spans
                .iter()
                .filter_map(|span| span.reading.as_deref())
                .collect::<String>();
            if joined.trim().is_empty() {
                None
            } else {
                Some(joined)
            }
        })
}

fn render_row<F>(groups: &[GroupCell], selector: F) -> String
where
    F: Fn(&GroupCell) -> Option<&str>,
{
    groups
        .iter()
        .map(|group| center_text(selector(group).unwrap_or(""), group.width))
        .collect::<String>()
        .trim_end()
        .to_string()
}

fn center_text(text: &str, width: usize) -> String {
    let content_width = display_width(text);
    if content_width >= width {
        return text.to_string();
    }

    let padding = width - content_width;
    let left = padding / 2;
    let right = padding - left;
    format!("{}{}{}", " ".repeat(left), text, " ".repeat(right))
}

pub fn display_width(text: &str) -> usize {
    text.chars()
        .map(|character| if character.is_ascii() { 1 } else { 2 })
        .sum()
}

#[cfg(test)]
mod tests {
    use crate::model::{
        NativeLanguage, RewriteStatus, SentenceRecord, SentenceToken, Settings, TokenSpan,
    };

    use super::{build_groups, center_text, display_width, render_sentence};

    fn sample_sentence() -> SentenceRecord {
        SentenceRecord {
            id: 1,
            lan: NativeLanguage::Chinese,
            source_text: "匿名さん、すごいですね。".to_string(),
            translated_text: "匿名的人真厉害啊。".to_string(),
            style: None,
            created_at_unix_secs: 0,
            updated_at_unix_secs: 0,
            romaji_line: "tokumei-san sugoi desu ne".to_string(),
            furigana_line: "とくめいさん すごい です ね".to_string(),
            tokens: vec![
                SentenceToken {
                    surface: "匿名さん".to_string(),
                    reading: Some("とくめいさん".to_string()),
                    romaji: Some("tokumei-san".to_string()),
                    lemma: Some("匿名さん".to_string()),
                    gloss: Some("匿名的人".to_string()),
                    analysis: Some("称呼匿名的人".to_string()),
                    context_gloss: Some("匿名的人".to_string()),
                    context_analysis: Some("称呼匿名的人".to_string()),
                    variants: vec!["匿名さん".to_string()],
                    spans: vec![
                        TokenSpan {
                            text: "匿名".to_string(),
                            reading: Some("とくめい".to_string()),
                        },
                        TokenSpan {
                            text: "さん".to_string(),
                            reading: None,
                        },
                    ],
                },
                SentenceToken {
                    surface: "、".to_string(),
                    reading: None,
                    romaji: None,
                    lemma: None,
                    gloss: None,
                    analysis: None,
                    context_gloss: None,
                    context_analysis: None,
                    variants: vec!["、".to_string()],
                    spans: vec![TokenSpan {
                        text: "、".to_string(),
                        reading: None,
                    }],
                },
                SentenceToken {
                    surface: "すごい".to_string(),
                    reading: Some("すごい".to_string()),
                    romaji: Some("sugoi".to_string()),
                    lemma: Some("すごい".to_string()),
                    gloss: Some("厉害".to_string()),
                    analysis: Some("形容词".to_string()),
                    context_gloss: Some("厉害".to_string()),
                    context_analysis: Some("形容词".to_string()),
                    variants: vec!["すごい".to_string()],
                    spans: vec![TokenSpan {
                        text: "すごい".to_string(),
                        reading: Some("すごい".to_string()),
                    }],
                },
                SentenceToken {
                    surface: "です".to_string(),
                    reading: Some("です".to_string()),
                    romaji: Some("desu".to_string()),
                    lemma: Some("です".to_string()),
                    gloss: Some("是".to_string()),
                    analysis: Some("礼貌语尾".to_string()),
                    context_gloss: Some("是".to_string()),
                    context_analysis: Some("礼貌语尾".to_string()),
                    variants: vec!["です".to_string()],
                    spans: vec![TokenSpan {
                        text: "です".to_string(),
                        reading: Some("です".to_string()),
                    }],
                },
                SentenceToken {
                    surface: "ね".to_string(),
                    reading: Some("ね".to_string()),
                    romaji: Some("ne".to_string()),
                    lemma: Some("ね".to_string()),
                    gloss: Some("呢".to_string()),
                    analysis: Some("句末助词".to_string()),
                    context_gloss: Some("呢".to_string()),
                    context_analysis: Some("句末助词".to_string()),
                    variants: vec!["ね".to_string()],
                    spans: vec![TokenSpan {
                        text: "ね".to_string(),
                        reading: Some("ね".to_string()),
                    }],
                },
                SentenceToken {
                    surface: "。".to_string(),
                    reading: None,
                    romaji: None,
                    lemma: None,
                    gloss: None,
                    analysis: None,
                    context_gloss: None,
                    context_analysis: None,
                    variants: vec!["。".to_string()],
                    spans: vec![TokenSpan {
                        text: "。".to_string(),
                        reading: None,
                    }],
                },
            ],
            word_ids: vec![1, 2, 3, 4],
            rewrite_status: RewriteStatus::Done,
            rewrite_error: None,
        }
    }

    #[test]
    fn display_width_uses_simple_fullwidth_halfwidth_rule() {
        assert_eq!(display_width("abc"), 3);
        assert_eq!(display_width("匿名"), 4);
        assert_eq!(display_width("です"), 4);
    }

    #[test]
    fn group_width_is_maximum_of_surface_and_annotations() {
        let groups = build_groups(&sample_sentence());
        assert_eq!(groups[0].width, display_width("とくめいさん"));
        assert_eq!(groups[2].width, display_width("すごい"));
        assert_eq!(groups[4].width, display_width("ね"));
    }

    #[test]
    fn center_text_uses_minimum_padding() {
        assert_eq!(center_text("abc", 5), " abc ");
        assert_eq!(center_text("匿名", 6), " 匿名 ");
    }

    #[test]
    fn punctuation_remains_visible_as_its_own_group() {
        let groups = build_groups(&sample_sentence());
        assert_eq!(groups[1].surface, "、");
        assert_eq!(groups[5].surface, "。");
    }

    #[test]
    fn render_obeys_visibility_settings() {
        let rendered = render_sentence(&sample_sentence(), &Settings::default());
        assert!(rendered.contains("匿名的人真厉害啊。"));
        assert!(rendered.contains("tokumei-san"));
        assert!(rendered.contains("とくめいさん"));
        assert!(rendered.contains("匿名さん"));
        assert!(rendered.contains("、"));
        assert!(rendered.contains("。"));
    }

    #[test]
    fn render_falls_back_when_tokens_are_missing() {
        let mut sentence = sample_sentence();
        sentence.tokens.clear();
        let rendered = render_sentence(&sentence, &Settings::default());
        assert!(rendered.contains("tokumei-san sugoi desu ne"));
        assert!(rendered.contains("とくめいさん すごい です ね"));
        assert!(rendered.contains("匿名さん、すごいですね。"));
    }
}
