use unicode_width::UnicodeWidthStr;

use crate::model::{SentenceRecord, Settings, TokenSpan};

#[derive(Debug, Clone)]
struct TokenLayout {
    surface: String,
    block_width: usize,
    romaji: Option<String>,
    spans: Vec<SpanLayout>,
    is_punctuation: bool,
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
    let mut aligned_rows = Vec::new();
    if settings.romaji_enabled {
        aligned_rows.push(render_romaji_row(&layouts));
    }
    if settings.furigana_enabled {
        aligned_rows.push(render_furigana_row(&layouts));
    }
    aligned_rows.push(render_surface_row(&layouts));
    lines.extend(trim_common_left_margin(&aligned_rows));
    lines.join("\n")
}

fn build_layouts(sentence: &SentenceRecord) -> Vec<TokenLayout> {
    let mut layouts = Vec::new();

    for token in &sentence.tokens {
        let surface_width = display_width(&token.surface);
        let romaji_width = token.romaji.as_deref().map(display_width).unwrap_or(0);
        let spans = if token.spans.is_empty() {
            vec![TokenSpan {
                text: token.surface.clone(),
                reading: token.reading.clone(),
            }]
        } else {
            token.spans.clone()
        };
        let block_width = required_block_width(&token.surface, romaji_width, &spans);
        let surface_start = (block_width.saturating_sub(surface_width)) / 2;

        let mut span_layouts = Vec::new();
        let mut span_offset = 0;
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
                start: surface_start + span_offset,
                reading: span.reading,
            });
            span_offset += span_width;
        }

        layouts.push(TokenLayout {
            surface: token.surface.clone(),
            block_width,
            romaji: token.romaji.clone(),
            spans: span_layouts,
            is_punctuation: is_punctuation(&token.surface),
        });
    }

    layouts
}

fn render_surface_row(layouts: &[TokenLayout]) -> String {
    let mut row = String::new();

    for (index, layout) in layouts.iter().enumerate() {
        if index > 0 && !layout.is_punctuation {
            row.push(' ');
        }

        row.push_str(&center_text_in_width(&layout.surface, layout.block_width));
    }

    row.trim_end().to_string()
}

fn render_romaji_row(layouts: &[TokenLayout]) -> String {
    render_annotation_row(layouts, |layout| {
        layout.romaji.as_deref().map(str::to_owned)
    })
}

fn render_furigana_row(layouts: &[TokenLayout]) -> String {
    let mut chunks = Vec::new();

    for layout in layouts {
        let mut cells = vec![Cell::Empty; layout.block_width];
        for span in &layout.spans {
            if let Some(reading) = span.reading.as_deref() {
                let reading = reading.trim();
                if !reading.is_empty() {
                    place_centered_text(&mut cells, reading, span.start, span.text_width);
                }
            }
        }
        chunks.push(render_overlay_row(&cells));
    }

    join_chunks(layouts, &chunks)
}

fn render_annotation_row<F>(layouts: &[TokenLayout], mut annotation: F) -> String
where
    F: FnMut(&TokenLayout) -> Option<String>,
{
    let mut chunks = Vec::new();

    for layout in layouts {
        let text = annotation(layout).unwrap_or_default();
        chunks.push(center_text_in_width(&text, layout.block_width));
    }

    join_chunks(layouts, &chunks)
}

fn join_chunks(layouts: &[TokenLayout], chunks: &[String]) -> String {
    let mut row = String::new();

    for (index, (layout, chunk)) in layouts.iter().zip(chunks.iter()).enumerate() {
        if index > 0 && !layout.is_punctuation {
            row.push(' ');
        }
        row.push_str(chunk);
    }

    row.trim_end().to_string()
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

fn center_text_in_width(text: &str, width: usize) -> String {
    let text_width = display_width(text);
    if width <= text_width {
        return text.to_string();
    }

    let left_padding = (width - text_width) / 2;
    let right_padding = width - text_width - left_padding;
    let mut rendered = String::new();
    rendered.push_str(&" ".repeat(left_padding));
    rendered.push_str(text);
    rendered.push_str(&" ".repeat(right_padding));
    rendered
}

fn trim_common_left_margin(rows: &[String]) -> Vec<String> {
    let margin = rows
        .iter()
        .filter(|row| !row.trim().is_empty())
        .map(|row| {
            row.chars()
                .take_while(|character| *character == ' ')
                .count()
        })
        .min()
        .unwrap_or(0);

    rows.iter()
        .map(|row| row.chars().skip(margin).collect::<String>())
        .collect()
}

fn required_block_width(surface: &str, romaji_width: usize, spans: &[TokenSpan]) -> usize {
    let surface_width = display_width(surface);
    let mut block_width = surface_width.max(romaji_width);

    loop {
        let surface_start = (block_width.saturating_sub(surface_width)) / 2;
        let mut span_offset = 0;
        let mut fits = true;

        for span in spans {
            let span_width = display_width(&span.text);
            if let Some(reading) = span.reading.as_deref() {
                let reading_width = display_width(reading.trim());
                if reading_width > 0 {
                    let start =
                        centered_start(surface_start + span_offset, span_width, reading_width);
                    if start < 0 || (start as usize + reading_width) > block_width {
                        fits = false;
                        break;
                    }
                }
            }
            span_offset += span_width;
        }

        if fits {
            return block_width.max(1);
        }

        block_width += 1;
    }
}

fn centered_start(target_start: usize, target_width: usize, text_width: usize) -> isize {
    if text_width > target_width {
        target_start as isize - ((text_width - target_width) / 2) as isize
    } else {
        (target_start + (target_width - text_width) / 2) as isize
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

    use super::{center_text_in_width, render_sentence, trim_common_left_margin};

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
        assert!(rendered.contains(" iku"));
        assert!(rendered.contains("とうきょう"));
        assert!(rendered.contains("東京"));
        assert!(rendered.contains("行く。"));
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

    #[test]
    fn center_text_expands_with_spaces_without_truncation() {
        assert_eq!(center_text_in_width("abc", 5), " abc ");
        assert_eq!(center_text_in_width("abcdef", 3), "abcdef");
    }

    #[test]
    fn trim_common_left_margin_shifts_all_annotation_rows_together() {
        let rows = vec![
            "  abc".to_string(),
            "  def".to_string(),
            "  ghi".to_string(),
        ];
        assert_eq!(
            trim_common_left_margin(&rows),
            vec!["abc".to_string(), "def".to_string(), "ghi".to_string()]
        );
    }
}
