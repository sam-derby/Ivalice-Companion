//! Resolve a saved current job's primary command from pinned Job Nex relations.

use super::{CatalogueRef, ReaderDocument};
use crate::ValueState;

pub fn apply_primary_commands(
    document: &mut ReaderDocument,
    resolve: impl Fn(&str) -> ValueState<CatalogueRef>,
) {
    let ValueState::Known(units) = &mut document.roster.value else {
        return;
    };
    for unit in units {
        if let ValueState::Known(job) = &unit.current_job.value {
            unit.abilities.primary_command.value = resolve(&job.id);
        }
    }
}
