//! Read-only inspection shared by startup and explicit offline transfer.
use super::{evidence::Evidence, sqlite::Store, validation, IqError, IqRun, Result};
use crate::shared::persistence::Database;
use std::{collections::HashSet, path::Path, sync::Arc};

/// Refuse unknown evidence formats before recovery can modify files.
pub fn inspect_evidence_formats(root: &Path) -> Result<()> {
    Evidence::new(root.to_owned()).validate_formats()
}

/// Decode immutable historical snapshots with the same reader used by IQ history.
/// Does not recover runs, prepare directories, or repair incomplete evidence.
pub fn verify_history(database: Arc<Database>, root: &Path) -> Result<Vec<IqRun>> {
    inspect_evidence_formats(root)?;
    let store = Store::new(database);
    store.questions()?;
    let runs = store.runs()?;
    for run in &runs {
        validation::identifier(&run.id, "runId")?;
        let mut positions = HashSet::new();
        if run.entries.len() != run.questions.len().saturating_mul(run.participants.len()) {
            return Err(IqError::Storage);
        }
        for entry in &run.entries {
            validation::identifier(&entry.id, "entryId")?;
            if entry.question_index >= run.questions.len()
                || entry.participant_index >= run.participants.len()
                || !positions.insert((entry.question_index, entry.participant_index))
            {
                return Err(IqError::Storage);
            }
        }
    }
    Ok(runs)
}
