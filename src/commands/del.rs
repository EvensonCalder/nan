use crate::error::NanError;
use crate::store::Store;

pub fn run(store: &Store, n: usize) -> Result<(), NanError> {
    let mut database = store.load_or_create()?;
    if n == 0 || n > database.sentences.len() {
        return Err(NanError::message(format!(
            "sentence index {n} is out of range"
        )));
    }

    let removed = database.sentences.remove(n - 1);
    for word in &mut database.words {
        word.source_sentence_ids
            .retain(|sentence_id| *sentence_id != removed.id);
    }
    database
        .words
        .retain(|word| !word.source_sentence_ids.is_empty());
    store.save(&database)?;

    println!("deleted sentence {n}: {}", removed.source_text);
    Ok(())
}
