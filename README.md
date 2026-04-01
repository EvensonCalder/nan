# nan

`nan` means "what" in Japanese (`何`).

This project is a sentence-first Japanese learning CLI written in Rust. Instead of course-style memorization, it helps you learn Japanese through natural sentences, AI-assisted analysis, and lightweight spaced review.

## Features

- `nan add [sentence] [style?]`
- `nan new [n?] [style?]`
- `nan cat [n?]`
- `nan list [n?] [word|sentence]`
- `nan del [n]`
- `nan set [ref|level|base-url|api-key|model|roomaji|furigana|lan] [option]`

All settings and learning data are stored in `~/.nanconfig.json`.

## Build

```bash
cargo build --release
```

The standalone binary will be available at:

```text
target/release/nan
```

## Quick Start

Configure the AI backend first:

```bash
nan set base-url https://api.openai.com/v1
nan set api-key YOUR_API_KEY
nan set model gpt-4o-mini
```

Environment variables are also supported and take priority over `~/.nanconfig.json`:

```bash
export NAN_OPENAI_BASE_URL=https://api.openai.com/v1
export NAN_OPENAI_API_KEY=YOUR_API_KEY
export NAN_OPENAI_MODEL=gpt-4o-mini
```

Priority order:

- `NAN_OPENAI_BASE_URL` over `nan set base-url ...`
- `NAN_OPENAI_API_KEY` over `nan set api-key ...`
- `NAN_OPENAI_MODEL` over `nan set model ...`

Optional learning settings:

```bash
nan set level n5.5
nan set ref 10
nan set roomaji on
nan set furigana on
nan set lan chinese
```

Then start learning:

```bash
nan add "今晚的月色真美" "Natsume Soseki"
nan new 3 "daily"
nan cat 5
nan list 10 sentence
nan list 20 word
nan del 2
```

## Command Semantics

### `nan add [sentence] [style?]`

- Translates the input into natural Japanese.
- Generates a native-language translation.
- Generates romaji and furigana.
- Splits the sentence into tokens.
- Produces learner-facing word glosses and short analyses.
- Deduplicates words by canonical form and variant set.

### `nan new [n?] [style?]`

- If the first optional argument is an integer, it is treated as `n`.
- Otherwise, it is treated as `style`.
- The command uses low-memory words as generation hints.
- It asks the AI for `2 * n` candidates, then keeps the first unique valid `n` results.

### `nan cat [n?]`

- Selects sentences that cover as many weak words as possible.
- Updates the review state of the words inside the selected sentences.

### `nan list [n?] [word|sentence]`

- Default target is `sentence`.
- Positive `n`: lowest-memory-first.
- Negative `n`: newest `-n` items.
- `word` output is unnumbered.
- `sentence` output is numbered.

### `nan del [n]`

- Deletes the sentence with index `n`.
- Sentence numbering closes automatically after deletion.
- Orphaned words are removed if no sentence references them anymore.

### `nan set ...`

- `ref`: reference capacity for `new`
- `level`: one of `n5.5/n5/n4.5/n4/n3.5/n3/n2.5/n2/n1.5/n1`
- `base-url`: OpenAI-compatible chat completions base URL
- `api-key`: API key
- `model`: model name
- `roomaji`: `on` or `off`
- `furigana`: `on` or `off`
- `lan`: `english` or `chinese`

## Data Model

The system is sentence-first.

- Sentences are stored in creation order.
- Display indexes are derived from the current array order.
- Deleting a sentence automatically shifts later indexes forward.
- Internal sentence and word references use stable numeric IDs.

Each sentence stores:

- Japanese text
- native-language translation
- `lan`
- optional style
- romaji
- furigana
- token analysis
- linked word IDs

Each word stores:

- canonical form
- translation
- short analysis
- variant forms
- linked sentence IDs
- `lan`
- spaced-review state

## Review Formula

The review state follows the requested model.

- Initial stability: `S0 = 0.018`
- `beta = 0.25`
- `a = 0.6`
- `b = 0.08`
- Internal timestamps use Unix seconds and are converted to days for the formula

`cat` updates each reviewed word with the configured stability update rule.

## Language Rewrite and Recovery

Changing `lan` triggers AI-based rewriting for:

- all sentence translations
- all word translations
- all word analyses

This process is resumable.

- The target language is stored in `~/.nanconfig.json`.
- Each sentence and word carries its own `lan` field.
- Rewrite progress is persisted after each successful rewritten item.
- If the process is interrupted, running `nan` again in an interactive terminal will let you choose a target language and continue.
- In non-interactive environments, `nan` fails fast with a recovery hint instead of blocking.

## Rendering

`nan` renders output in this order:

1. Translation
2. Romaji
3. Furigana
4. Japanese sentence

The renderer uses token-level display width calculations and centers romaji and furigana as closely as possible over the related token spans.

## Quality Checks

The project is kept clean with:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

## Notes

- The AI backend must support an OpenAI-compatible `chat/completions` endpoint.
- The prompts require strict JSON responses.
- The current renderer is terminal-oriented and width-aware, but exact alignment can still vary slightly across fonts and terminals.
