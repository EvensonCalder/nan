use unicode_width::UnicodeWidthStr;

use crate::model::{SentenceRecord, Settings, TokenSpan};

#[derive(Debug, Clone)]
struct TokenLayout {
    surface: String,
    surface_start: usize,
    surface_width: usize,
    romaji: Option<String>,
    spans: Vec<SpanLayout>,
}

#[derive(Debug, Clone)]
struct SpanLayout {
    text_width: usize,
    start: usize,
    reading: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Cell {
    Empty,
    Continuation,
    Text(String),
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

    let layouts = build_layouts(sentence);
    if settings.romaji_enabled {
        lines.push(render_romaji_row(&layouts));
    }
    if settings.furigana_enabled {
        lines.push(render_furigana_row(&layouts));
    }
    lines.push(render_surface_row(&layouts));
    lines.join("\n")
}

fn build_layouts(sentence: &SentenceRecord) -> Vec<TokenLayout> {
    let mut layouts = Vec::new();
    let mut current_column = 0;

    for token in &sentence.tokens {
        let gap = if layouts.is_empty() || is_punctuation(&token.surface) {
            0
        } else {
            1
        };
        current_column += gap;

        let surface_width = display_width(&token.surface);
        let surface_start = current_column;
        let mut span_layouts = Vec::new();
        let mut span_column = surface_start;
        let spans = if token.spans.is_empty() {
            vec![TokenSpan {
                text: token.surface.clone(),
                reading: token.reading.clone(),
            }]
        } else {
            token.spans.clone()
        };

        for span in spans {
            let span_width = display_width(&span.text);
            span_layouts.push(SpanLayout {
                text_width: span_width,
                start: span_column,
                reading: span.reading,
            });
            span_column += span_width;
        }

        layouts.push(TokenLayout {
            surface: token.surface.clone(),
            surface_start,
            surface_width,
            romaji: token.romaji.clone(),
            spans: span_layouts,
        });
        current_column = surface_start + surface_width;
    }

    layouts
}

fn render_surface_row(layouts: &[TokenLayout]) -> String {
    let mut row = String::new();
    let mut current_width = 0;

    for layout in layouts {
        pad_to_width(&mut row, &mut current_width, layout.surface_start);
        row.push_str(&layout.surface);
        current_width += layout.surface_width;
    }

    row
}

fn render_romaji_row(layouts: &[TokenLayout]) -> String {
    let mut row = Vec::new();
    for layout in layouts {
        if let Some(romaji) = layout.romaji.as_deref() {
            place_centered_text(&mut row, romaji, layout.surface_start, layout.surface_width);
        }
    }
    render_overlay_row(&row)
}

fn render_furigana_row(layouts: &[TokenLayout]) -> String {
    let mut row = Vec::new();
    for layout in layouts {
        for span in &layout.spans {
            if let Some(reading) = span.reading.as_deref()
                && !reading.trim().is_empty()
            {
                place_centered_text(&mut row, reading, span.start, span.text_width);
            }
        }
    }
    render_overlay_row(&row)
}

fn place_centered_text(row: &mut Vec<Cell>, text: &str, target_start: usize, target_width: usize) {
    let text_width = display_width(text);
    if text_width == 0 {
        return;
    }

    let mut start = if text_width > target_width {
        target_start.saturating_sub((text_width - target_width) / 2)
    } else {
        target_start + (target_width - text_width) / 2
    };

    while overlaps(row, start, text_width) {
        start += 1;
    }

    ensure_len(row, start + text_width);
    row[start] = Cell::Text(text.to_string());
    for cell in row.iter_mut().take(start + text_width).skip(start + 1) {
        *cell = Cell::Continuation;
    }
}

fn overlaps(row: &[Cell], start: usize, width: usize) -> bool {
    if width == 0 {
        return false;
    }

    for index in start..(start + width) {
        if let Some(cell) = row.get(index)
            && !matches!(cell, Cell::Empty)
        {
            return true;
        }
    }

    false
}

fn ensure_len(row: &mut Vec<Cell>, len: usize) {
    if row.len() < len {
        row.resize(len, Cell::Empty);
    }
}

fn render_overlay_row(row: &[Cell]) -> String {
    let mut rendered = String::new();
    for cell in row {
        match cell {
            Cell::Empty => rendered.push(' '),
            Cell::Continuation => {}
            Cell::Text(text) => rendered.push_str(text),
        }
    }

    rendered.trim_end().to_string()
}

fn pad_to_width(buffer: &mut String, current_width: &mut usize, target_width: usize) {
    while *current_width < target_width {
        buffer.push(' ');
        *current_width += 1;
    }
}

fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width_cjk(text)
}

fn is_punctuation(text: &str) -> bool {
    text.chars().all(|character| {
        matches!(
            character,
            '。' | '、' | '！' | '？' | '.' | ',' | '!' | '?' | ':' | ';' | '，' | '：' | '；'
        )
    })
}

#[cfg(test)]
mod tests {
    use crate::model::{
        NativeLanguage, RewriteStatus, SentenceRecord, SentenceToken, Settings, TokenSpan,
    };

    use super::render_sentence;

    fn sample_sentence() -> SentenceRecord {
        SentenceRecord {
            id: 1,
            lan: NativeLanguage::Chinese,
            source_text: "東京へ行く。".to_string(),
            translated_text: "去东京。".to_string(),
            style: None,
            created_at_unix_secs: 0,
            updated_at_unix_secs: 0,
            romaji_line: "toukyou e iku.".to_string(),
            furigana_line: "とうきょう へ いく。".to_string(),
            tokens: vec![
                SentenceToken {
                    surface: "東京".to_string(),
                    reading: Some("とうきょう".to_string()),
                    romaji: Some("toukyou".to_string()),
                    lemma: Some("東京".to_string()),
                    gloss: Some("Tokyo".to_string()),
                    variants: vec!["東京".to_string()],
                    spans: vec![TokenSpan {
                        text: "東京".to_string(),
                        reading: Some("とうきょう".to_string()),
                    }],
                },
                SentenceToken {
                    surface: "へ".to_string(),
                    reading: Some("へ".to_string()),
                    romaji: Some("e".to_string()),
                    lemma: Some("へ".to_string()),
                    gloss: Some("to".to_string()),
                    variants: vec!["へ".to_string()],
                    spans: vec![TokenSpan {
                        text: "へ".to_string(),
                        reading: Some("へ".to_string()),
                    }],
                },
                SentenceToken {
                    surface: "行く".to_string(),
                    reading: Some("いく".to_string()),
                    romaji: Some("iku".to_string()),
                    lemma: Some("行く".to_string()),
                    gloss: Some("go".to_string()),
                    variants: vec!["行く".to_string()],
                    spans: vec![
                        TokenSpan {
                            text: "行".to_string(),
                            reading: Some("い".to_string()),
                        },
                        TokenSpan {
                            text: "く".to_string(),
                            reading: None,
                        },
                    ],
                },
                SentenceToken {
                    surface: "。".to_string(),
                    reading: None,
                    romaji: None,
                    lemma: None,
                    gloss: None,
                    variants: vec!["。".to_string()],
                    spans: vec![TokenSpan {
                        text: "。".to_string(),
                        reading: None,
                    }],
                },
            ],
            word_ids: vec![1, 2, 3],
            rewrite_status: RewriteStatus::Done,
            rewrite_error: None,
        }
    }

    #[test]
    fn render_obeys_visibility_settings() {
        let sentence = sample_sentence();
        let settings = Settings::default();
        let rendered = render_sentence(&sentence, &settings);
        assert!(rendered.contains("去东京。"));
        assert!(rendered.contains("toukyou"));
        assert!(rendered.contains("とうきょう"));
        assert!(
            rendered.contains("東京へ 行く。")
                || rendered.contains("東京 へ 行く。")
                || rendered.contains("東京 へ行く。")
                || rendered.contains("東京へ行く。")
        );
    }

    #[test]
    fn render_falls_back_when_tokens_are_missing() {
        let mut sentence = sample_sentence();
        sentence.tokens.clear();
        let rendered = render_sentence(&sentence, &Settings::default());
        assert!(rendered.contains("toukyou e iku."));
        assert!(rendered.contains("とうきょう へ いく。"));
        assert!(rendered.contains("東京へ行く。"));
    }
}
