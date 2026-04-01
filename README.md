# nan

`nan` means `何` in Japanese: "what".

`nan` is a sentence-first Japanese learning CLI written in Rust. It is designed for natural learning instead of course-style drilling. The core idea is simple:

- learn Japanese through complete sentences
- let AI produce natural Japanese, token analysis, romaji, and furigana
- track memory on attached words instead of isolated word lists
- review by selecting sentences that best cover weak words

All persistent data is stored in a single file:

```text
~/.nanconfig.json
```

## Build

```bash
cargo build --release
```

The standalone binary is:

```text
target/release/nan
```

## Configuration

`nan` supports both config-file settings and environment variables.

Environment variables have higher priority than `~/.nanconfig.json`:

```bash
export NAN_OPENAI_BASE_URL=https://api.openai.com/v1
export NAN_OPENAI_API_KEY=YOUR_API_KEY
export NAN_OPENAI_MODEL=gpt-4o-mini
```

Config commands:

```bash
nan set base-url https://api.openai.com/v1
nan set api-key YOUR_API_KEY
nan set model gpt-4o-mini
nan set ref 10
nan set level n5.5
nan set roomaji on
nan set furigana on
nan set lan chinese
```

Settings meaning:

- `ref`: how many weak words are used as the reference capacity for `new`
- `level`: target learner level used to guide AI output
- `base-url`: OpenAI-compatible `chat/completions` base URL
- `api-key`: API key for the chat backend
- `model`: model name
- `roomaji`: display toggle only; romaji is still fetched and stored
- `furigana`: display toggle only; furigana is still fetched and stored
- `lan`: native-language target for sentence translations and word analyses

## Commands

### `nan add [sentence] [style?]`

Example:

```bash
nan add "今晚的月色真美" "Natsume Soseki"
```

Behavior:

- takes a source sentence in the user language
- asks AI for a natural Japanese sentence
- optionally nudges the output toward a style
- stores:
  - Japanese sentence
  - native-language translation
  - romaji line
  - furigana line
  - token analysis
  - linked word IDs
- deduplicates words by canonical form and known variants

### `nan new [n?] [style?]`

Examples:

```bash
nan new
nan new 3
nan new "daily"
nan new 3 "daily"
```

Argument rule:

- if the first optional argument is an integer, it is treated as `n`
- otherwise it is treated as `style`

Behavior:

- finds low-memory words
- sends those weak words and related old sentences to AI as generation references
- asks AI for `2 * n` candidates
- filters out:
  - exact duplicates
  - highly similar near-rephrasings based on token word overlap
- stores the accepted new sentences and their words

### `nan cat [n?]`

Example:

```bash
nan cat 5
```

Behavior:

- selects sentences that cover as many weak words as possible
- prints `n` review sentences
- updates review state for the words contained in those sentences

### `nan list [n?] [word|sentence]`

Examples:

```bash
nan list
nan list 10 sentence
nan list 20 word
nan list -5 sentence
nan list -20 word
```

Behavior:

- default target is `sentence`
- positive `n` means lowest-memory-first
- negative `n` means newest `-n` items
- `word` mode prints aligned word / translation / analysis rows
- `sentence` mode prints aligned index / sentence / translation rows

### `nan del [n]`

Example:

```bash
nan del 2
```

Behavior:

- deletes the sentence with display index `n`
- display indexes close automatically after deletion
- removes orphaned words that are no longer referenced by any sentence

### `nan set [key] [option]`

Supported keys:

- `ref`
- `level`
- `base-url`
- `api-key`
- `model`
- `roomaji`
- `furigana`
- `lan`

## Data Model

`nan` is sentence-first.

That means:

- sentences are the primary learning objects
- words are attached to sentences
- review priority is computed from words
- generation and review both use sentence context instead of isolated flashcard logic

### Sentence Records

Sentences are stored in insertion order.

Display indexes are derived from the current array order:

- the first sentence is shown as `1`
- after deletion, later sentences shift forward automatically

Each sentence stores:

- stable internal `id`
- `lan`
- Japanese text
- translated text
- optional style
- romaji line
- furigana line
- token list
- linked word IDs
- rewrite status fields

### Word Records

Words are deduplicated independently from sentence display indexes.

Each word stores:

- stable internal `id`
- `lan`
- canonical form
- translation
- learner-facing analysis
- known variants
- source sentence IDs
- memory state
- rewrite status fields

Word records are intended to represent a word family rather than just one surface form.

## Memory Logic

The review system tracks memory on words, not on whole sentences.

Initial stability:

- `S0 = 0.018`

Parameters:

- `beta = 0.25`
- `a = 0.6`
- `b = 0.08`

Time is stored internally in Unix seconds and converted into days for the review formula.

When `cat` reviews a sentence, all words attached to that sentence are updated with the configured stability equation.

Lower memory score means higher review priority.

## Generation Logic

### `add`

`add` asks AI for a single structured JSON result containing:

- Japanese sentence
- translation in `lan`
- romaji
- furigana
- token breakdown
- per-token gloss and analysis in `lan`
- variant forms for deduplication

The local logic then:

- normalizes variants
- reuses existing words when variants overlap
- creates new words when no existing match is found
- links the sentence to all matched word records

### `new`

`new` works in two phases.

Phase 1: choose references.

- rank words by memory score
- take the weakest `n * ref`
- gather related old sentences

Phase 2: filter AI candidates.

- reject exact sentence duplicates
- reject highly similar candidates using token-level word overlap
- keep only distinct, useful new sentences

This keeps `new` from filling the database with tiny paraphrases of the same sentence.

## Review Logic

`cat` is coverage-oriented.

It does not choose sentences randomly by default. Instead, it prefers sentences that cover weak words efficiently.

The selection logic tries to maximize:

- uncovered weak-word coverage first
- total weak-word weight second

This helps a small number of review sentences touch more weak vocabulary.

## Rendering Logic

Output is rendered in this order:

1. translation
2. romaji
3. furigana
4. Japanese sentence

Rendering rules:

- alignment is the primary goal
- extra blank space is acceptable when needed for alignment
- text must not be compressed into misalignment

The renderer uses token-level block widths.

For each token:

- compute the width needed by the Japanese token itself
- compute the width needed by romaji
- compute the width needed by furigana spans
- expand the token block until annotations fit cleanly

Then the rows are joined without forcing text to overlap.

`roomaji` and `furigana` settings only affect display.
The data is still requested from AI and stored even when those toggles are off.

## Language Rewrite Logic

Changing `lan` does not just update one setting.
It triggers a resumable rewrite process.

What gets rewritten:

- sentence translations
- word translations
- word analyses
- sentence-side token glosses, synchronized from the rewritten word data

How it works:

- target `lan` is written into settings
- sentence and word rewrite status is tracked per record
- each successful item is saved immediately
- interruption is recoverable

Startup behavior:

- if stored sentence/word languages are inconsistent with settings
- interactive runs ask the user to choose a target language and continue rewriting
- non-interactive runs fail immediately with a recovery hint instead of blocking

## AI Compatibility Logic

`nan` expects an OpenAI-compatible `chat/completions` API.

Compatibility decisions:

- environment variables override config file values
- structured reasoning fields are ignored
- only assistant `content` is parsed as result payload
- transient request failures are retried automatically

Current retry targets:

- `408`
- `409`
- `425`
- `429`
- `5xx`
- transport/network errors

The prompts require strict JSON output so local parsing stays deterministic.

## Storage File

Everything lives in one JSON file:

- settings
- sentences
- words
- rewrite progress
- schema version

This keeps backup, inspection, and migration simple.

## Notes

- The entire codebase is written in English.
- The binary is intended to work well on macOS.
- Terminal alignment still depends slightly on the terminal font, but the layout logic is width-aware and annotation-safe.
