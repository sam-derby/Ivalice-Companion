use super::*;
use std::collections::BTreeSet;

const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReaderError {
    TooLarge,
    InvalidJson,
    InvalidIdentity,
    InvalidText,
    InvalidValue,
    DuplicateIdentity,
    MissingReference,
    Stale,
}

/// The only publishable reader form. Neither deserialization nor a public DTO
/// constructor bypasses validation; no mutable document is exposed afterward.
#[derive(Clone, Debug)]
pub struct ValidatedReader(ReaderDocument);

impl ValidatedReader {
    pub fn from_json(bytes: &[u8]) -> Result<Self, ReaderError> {
        if bytes.len() > MAX_READER_BYTES {
            return Err(ReaderError::TooLarge);
        }
        let document = serde_json::from_slice(bytes).map_err(|_| ReaderError::InvalidJson)?;
        Self::validate(document)
    }

    pub fn validate(document: ReaderDocument) -> Result<Self, ReaderError> {
        identity(&document.identity)?;
        let validator = Validator;
        validator.fact(&document.roster, |units| {
            limited(units, MAX_READER_UNITS)?;
            let mut keys = BTreeSet::new();
            let mut persistent = BTreeSet::new();
            for unit in units {
                unique(&mut keys, unit.key)?;
                if let ValueState::Known(id) = &unit.persistent_identity.value {
                    unique(&mut persistent, id.as_str())?;
                }
                validator.unit(unit)?;
            }
            Ok(())
        })?;
        validator.fact(&document.inventory, |holdings| {
            limited(holdings, MAX_READER_ITEMS)?;
            let mut keys = BTreeSet::new();
            let mut items = BTreeSet::new();
            for holding in holdings {
                unique(&mut keys, holding.key)?;
                unique_catalogue(&mut items, &holding.item)?;
                validator.catalogue(&holding.item)?;
                validator.text(&holding.category)?;
                validator.scalar(&holding.quantity)?;
            }
            Ok(())
        })?;
        if document.item_details.len() > MAX_READER_ITEMS {
            return Err(ReaderError::TooLarge);
        }
        for (key, details) in &document.item_details {
            if !key.strip_prefix("item:").is_some_and(|number| {
                number
                    .parse::<u16>()
                    .is_ok_and(|id| key == &format!("item:{id}"))
            }) {
                return Err(ReaderError::InvalidValue);
            }
            limited(details, 32)?;
            for detail in details {
                if detail.is_empty() || detail.len() > 512 || detail.chars().any(char::is_control) {
                    return Err(ReaderError::InvalidText);
                }
            }
        }
        validator.fact(&document.gil, safe_integer)?;
        validator.progress(&document.progress)?;
        let result = Self(document);
        result.to_json()?;
        Ok(result)
    }

    #[must_use]
    pub fn document(&self) -> &ReaderDocument {
        &self.0
    }

    pub fn to_json(&self) -> Result<Vec<u8>, ReaderError> {
        let mut output = BoundedJson::default();
        serde_json::to_writer(&mut output, &self.0).map_err(|_| {
            if output.full {
                ReaderError::TooLarge
            } else {
                ReaderError::InvalidJson
            }
        })?;
        Ok(output.bytes)
    }

    /// Both generations, resource identity, slot and session must still match.
    pub fn for_identity(&self, expected: &ReaderIdentity) -> Result<&ReaderDocument, ReaderError> {
        identity(expected)?;
        if &self.0.identity != expected {
            return Err(ReaderError::Stale);
        }
        Ok(&self.0)
    }
}

struct Validator;

impl Validator {
    fn fact<T>(
        &self,
        fact: &Fact<T>,
        check: impl FnOnce(&T) -> Result<(), ReaderError>,
    ) -> Result<(), ReaderError> {
        known(&fact.value, check)
    }

    fn scalar<T>(&self, fact: &Fact<T>) -> Result<(), ReaderError> {
        self.fact(fact, |_| Ok(()))
    }

    fn text(&self, fact: &Fact<String>) -> Result<(), ReaderError> {
        self.fact(fact, |value| text(value, 256))
    }

    fn catalogue(&self, fact: &Fact<CatalogueRef>) -> Result<(), ReaderError> {
        self.fact(fact, catalogue)
    }

    fn loadout(&self, value: &AbilityLoadout) -> Result<(), ReaderError> {
        for fact in [
            &value.primary_command,
            &value.secondary_command,
            &value.reaction,
            &value.support,
            &value.movement,
        ] {
            self.catalogue(fact)?;
        }
        Ok(())
    }

    fn unit(&self, unit: &ReaderUnit) -> Result<(), ReaderError> {
        self.fact(&unit.persistent_identity, |value| token(value))?;
        {
            let saved = &unit.saved;
            bounded(saved.pad2.len() == 38)?;
            bounded(
                saved
                    .combat_sets
                    .iter()
                    .all(|set| set.name_padding.len() == 50),
            )?;
            bounded(saved.hp_max_base <= 0x00ff_ffff)?;
            bounded(saved.mp_max_base <= 0x00ff_ffff)?;
            bounded(saved.wt_base <= 0x00ff_ffff)?;
            bounded(saved.at_base <= 0x00ff_ffff)?;
            bounded(saved.mat_base <= 0x00ff_ffff)?;
            bounded(saved.unlocked_jobs <= 0x00ff_ffff)?;
            bounded(saved.job_levels.iter().enumerate().all(|(index, level)| {
                let packed = saved.job_levels_raw[index / 2];
                *level
                    == if index % 2 == 0 {
                        packed >> 4
                    } else {
                        packed & 0x0f
                    }
            }))?;
        }
        self.fact(&unit.name, |name| text(&name.text, 256))?;
        self.scalar(&unit.kind)?;
        self.scalar(&unit.membership)?;
        self.fact(&unit.sprite, |value| token(value))?;
        self.fact(&unit.portrait, |value| token(value))?;
        let stored = &unit.stored;
        for fact in [
            &stored.level,
            &stored.experience,
            &stored.brave,
            &stored.faith,
        ] {
            self.scalar(fact)?;
        }
        for fact in [&stored.sex, &stored.birthday, &stored.zodiac] {
            self.text(fact)?;
        }
        self.fact(&stored.statuses, |statuses| {
            limited(statuses, 128)?;
            let mut ids = BTreeSet::new();
            for status in statuses {
                catalogue(status)?;
                unique(&mut ids, &status.id)?;
            }
            Ok(())
        })?;
        let bases = &stored.bases;
        for fact in [
            &bases.hp,
            &bases.mp,
            &bases.speed,
            &bases.physical_attack,
            &bases.magical_attack,
        ] {
            self.fact(fact, |value| bounded(*value <= 0x00ff_ffff))?;
        }
        let stats = &unit.effective;
        for fact in [
            &stats.hp,
            &stats.mp,
            &stats.speed,
            &stats.physical_attack,
            &stats.magical_attack,
        ] {
            self.scalar(fact)?;
        }
        self.scalar(&stats.movement_tiles)?;
        self.scalar(&stats.jump_tiles)?;
        for (kind, detail) in &stats.breakdown {
            bounded(detail.maximum == kind.display_limit())?;
            self.fact(&detail.base, |value| bounded(*value > 0))?;
            self.scalar(&detail.equipment_bonus)?;
        }
        self.scalar(&unit.growth)?;
        self.fact(&stats.evasion, |entries| {
            limited(entries, 6)?;
            let mut kinds = BTreeSet::new();
            for entry in entries {
                unique(&mut kinds, entry.source)?;
                self.fact(&entry.physical_basis_points, |value| {
                    bounded(*value <= 10_000)
                })?;
                self.fact(&entry.magical_basis_points, |value| {
                    bounded(*value <= 10_000)
                })?;
            }
            Ok(())
        })?;
        self.catalogue(&unit.current_job)?;
        self.fact(&unit.jobs, |jobs| {
            limited(jobs, MAX_READER_JOBS)?;
            let mut ids = BTreeSet::new();
            for job in jobs {
                bounded(job.slot < 23)?;
                unique_catalogue(&mut ids, &job.job)?;
                self.catalogue(&job.job)?;
                self.scalar(&job.level)?;
                self.scalar(&job.current_jp)?;
                self.scalar(&job.total_jp)?;
            }
            Ok(())
        })?;
        self.fact(&unit.learned_abilities, |abilities| {
            limited(abilities, MAX_READER_ABILITIES)?;
            let mut ids = BTreeSet::new();
            for ability in abilities {
                unique_catalogue(&mut ids, &ability.ability)?;
                self.catalogue(&ability.ability)?;
                self.scalar(&ability.learned)?;
                self.scalar(&ability.jp_cost)?;
            }
            Ok(())
        })?;
        self.fact(&unit.saved_ability_flags, |groups| {
            limited(groups, 22)?;
            let mut slots = BTreeSet::new();
            for group in groups {
                bounded(group.slot < 22)?;
                unique(&mut slots, group.slot)?;
                bounded(group.active_positions.len() <= 16 && group.passive_positions.len() <= 8)?;
                bounded(group.active_positions.iter().all(|position| *position < 16))?;
                bounded(group.passive_positions.iter().all(|position| *position < 8))?;
                bounded(
                    group
                        .active_positions
                        .windows(2)
                        .all(|pair| pair[0] < pair[1]),
                )?;
                bounded(
                    group
                        .passive_positions
                        .windows(2)
                        .all(|pair| pair[0] < pair[1]),
                )?;
            }
            Ok(())
        })?;
        self.loadout(&unit.abilities)?;
        let equipment = &unit.equipment;
        for fact in [
            &equipment.head,
            &equipment.body,
            &equipment.accessory,
            &equipment.right_weapon,
            &equipment.right_shield,
            &equipment.left_weapon,
            &equipment.left_shield,
        ] {
            self.catalogue(fact)?;
        }
        self.fact(&unit.combat_sets, |sets| {
            limited(sets, 3)?;
            let mut ids = BTreeSet::new();
            for set in sets {
                bounded(set.key < 3)?;
                unique(&mut ids, set.key)?;
                self.scalar(&set.assigned)?;
                self.text(&set.name)?;
                for fact in [
                    &set.job,
                    &set.head,
                    &set.body,
                    &set.accessory,
                    &set.right_hand,
                    &set.left_hand,
                ] {
                    self.catalogue(fact)?;
                }
                self.loadout(&set.abilities)?;
                self.scalar(&set.double_hand)?;
            }
            Ok(())
        })?;
        self.fact(&unit.selected_combat_set, |selected| {
            bounded(*selected < 3)?;
            match &unit.combat_sets.value {
                ValueState::Known(sets) if sets.iter().any(|set| set.key == *selected) => Ok(()),
                _ => Err(ReaderError::MissingReference),
            }
        })?;
        let guidance = &unit.guidance;
        self.catalogue(&guidance.target_job)?;
        self.fact(&guidance.samurai_prerequisites, |value| {
            let levels = crate::SamuraiPrerequisiteLevels {
                squire: value.squire.current_level,
                knight: value.knight.current_level,
                archer: value.archer.current_level,
                monk: value.monk.current_level,
                thief: value.thief.current_level,
                dragoon: value.dragoon.current_level,
            };
            bounded(&crate::compare_samurai_prerequisites(&levels) == value)
        })?;
        self.fact(&guidance.steel_cost, |value| {
            let cost = crate::selected_steel_cost_progress(0).cost_jp;
            bounded(value.cost_jp == cost && value.shortfall_jp <= cost)?;
            bounded(match value.state {
                crate::NumericCostState::Enough => value.shortfall_jp == 0,
                crate::NumericCostState::Shortfall => value.shortfall_jp > 0,
            })
        })?;
        self.scalar(&guidance.job_eligible)?;
        self.scalar(&guidance.ability_purchasable)
    }

    fn progress(&self, progress: &SavedProgress) -> Result<(), ReaderError> {
        self.text(&progress.title)?;
        self.fact(&progress.saved_at_unix_seconds, |value| {
            safe_integer(&value.unsigned_abs())
        })?;
        self.text(&progress.hero_name)?;
        self.catalogue(&progress.location)?;
        self.catalogue(&progress.difficulty)?;
        self.scalar(&progress.difficulty_code)?;
        self.text(&progress.chapter)?;
        self.scalar(&progress.story_progress)?;
        self.text(&progress.objective)?;
        self.scalar(&progress.area_index)?;
        self.scalar(&progress.ramza_level)?;
        self.fact(&progress.play_time_seconds, safe_integer)?;
        self.scalar(&progress.next_event_id)?;
        self.fact(&progress.unnamed_event_values, |value| {
            bounded(*value <= 256)
        })?;
        for list in [
            &progress.story,
            &progress.errands,
            &progress.events,
            &progress.recruitment,
        ] {
            self.fact(list, |entries| {
                limited(entries, MAX_PROGRESS_ENTRIES)?;
                let mut ids = BTreeSet::new();
                for entry in entries {
                    unique_catalogue(&mut ids, &entry.subject)?;
                    self.catalogue(&entry.subject)?;
                    self.text(&entry.saved_state)?;
                }
                Ok(())
            })?;
        }
        Ok(())
    }
}

fn identity(value: &ReaderIdentity) -> Result<(), ReaderError> {
    let valid = token(&value.session).is_ok()
        && (1..=MAX_SAFE_INTEGER).contains(&value.snapshot_generation)
        && (1..=MAX_SAFE_INTEGER).contains(&value.resource_generation)
        && value.manual_slot < 50;
    if !valid {
        return Err(ReaderError::InvalidIdentity);
    }
    known(&value.resource_token, |value| token(value)).map_err(|_| ReaderError::InvalidIdentity)
}

fn known<T>(
    state: &ValueState<T>,
    check: impl FnOnce(&T) -> Result<(), ReaderError>,
) -> Result<(), ReaderError> {
    if let ValueState::Known(value) = state {
        check(value)?;
    }
    Ok(())
}

fn catalogue(value: &CatalogueRef) -> Result<(), ReaderError> {
    token(&value.id)?;
    bounded(
        value
            .id
            .split_once(':')
            .is_some_and(|(namespace, key)| !namespace.is_empty() && !key.is_empty()),
    )?;
    known(&value.label, |label| text(label, 256))?;
    known(&value.description, |description| {
        text_with_line_feeds(description, 4096, true)
    })?;
    known(&value.asset_key, |key| token(key))
}

fn text(value: &str, max: usize) -> Result<(), ReaderError> {
    text_with_line_feeds(value, max, false)
}

fn text_with_line_feeds(
    value: &str,
    max: usize,
    allow_line_feeds: bool,
) -> Result<(), ReaderError> {
    // Pinned item descriptions contain LF; names and tokens remain single-line.
    if value.trim().is_empty()
        || value.len() > max
        || value
            .chars()
            .any(|character| character.is_control() && !(allow_line_feeds && character == '\n'))
    {
        return Err(ReaderError::InvalidText);
    }
    Ok(())
}

fn token(value: &str) -> Result<(), ReaderError> {
    text(value, 128)?;
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:".contains(&byte))
    {
        return Err(ReaderError::InvalidText);
    }
    Ok(())
}

fn unique<T: Ord>(seen: &mut BTreeSet<T>, value: T) -> Result<(), ReaderError> {
    if !seen.insert(value) {
        return Err(ReaderError::DuplicateIdentity);
    }
    Ok(())
}

fn unique_catalogue<'a>(
    seen: &mut BTreeSet<&'a str>,
    value: &'a Fact<CatalogueRef>,
) -> Result<(), ReaderError> {
    if let ValueState::Known(reference) = &value.value {
        unique(seen, reference.id.as_str())?;
    }
    Ok(())
}

fn limited<T>(values: &[T], max: usize) -> Result<(), ReaderError> {
    if values.len() > max {
        return Err(ReaderError::TooLarge);
    }
    Ok(())
}

fn bounded(valid: bool) -> Result<(), ReaderError> {
    if valid {
        Ok(())
    } else {
        Err(ReaderError::InvalidValue)
    }
}

fn safe_integer(value: &u64) -> Result<(), ReaderError> {
    bounded(*value <= MAX_SAFE_INTEGER)
}

/// Memory-only serialization sink: reject before growing past the wire cap.
struct BoundedJson {
    bytes: Vec<u8>,
    full: bool,
}

impl Default for BoundedJson {
    fn default() -> Self {
        Self {
            bytes: Vec::with_capacity(MAX_READER_BYTES),
            full: false,
        }
    }
}

impl std::io::Write for BoundedJson {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        if buffer.len() > MAX_READER_BYTES.saturating_sub(self.bytes.len()) {
            self.full = true;
            return Err(std::io::Error::other("reader_output_limit"));
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn output_limit_is_enforced_before_memory_growth() {
        let mut output = BoundedJson::default();
        let first = MAX_READER_BYTES / 2 + 1;
        assert!(output.write_all(&vec![b'a'; first]).is_ok());
        assert!(output.write_all(b"b").is_ok());
        assert!(output
            .write_all(&vec![b'c'; MAX_READER_BYTES - first - 1])
            .is_ok());
        assert!(output.write_all(b"b").is_err());
        assert!(output.full);
        assert_eq!(output.bytes.len(), MAX_READER_BYTES);
        assert!(output.bytes.capacity() <= MAX_READER_BYTES);
    }
}
