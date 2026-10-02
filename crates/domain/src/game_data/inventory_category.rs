//! Broad inventory filters from TICSaveEditor `Resources/Nex/en/Item.json`
//! at 07ea857, using `Key` and `UiItemCategoryId` for all 261 rows.

/// The game groups item keys by category, with five later additions at 256..260.
/// Zero and 254..255 have no item category in the pinned table.
#[must_use]
pub const fn inventory_category(item_id: u16) -> Option<&'static str> {
    match item_id {
        1..=127 | 256..=257 => Some("Weapons"),
        128..=143 => Some("Shields"),
        144..=171 | 258 => Some("Headgear"),
        172..=207 | 259 => Some("Armor"),
        208..=239 | 260 => Some("Accessories"),
        240..=253 => Some("Consumables"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundaries_and_later_additions() {
        for (key, expected) in [
            (0, None),
            (1, Some("Weapons")),
            (127, Some("Weapons")),
            (128, Some("Shields")),
            (143, Some("Shields")),
            (144, Some("Headgear")),
            (171, Some("Headgear")),
            (172, Some("Armor")),
            (207, Some("Armor")),
            (208, Some("Accessories")),
            (239, Some("Accessories")),
            (240, Some("Consumables")),
            (253, Some("Consumables")),
            (254, None),
            (255, None),
            (256, Some("Weapons")),
            (257, Some("Weapons")),
            (258, Some("Headgear")),
            (259, Some("Armor")),
            (260, Some("Accessories")),
            (261, None),
        ] {
            assert_eq!(inventory_category(key), expected, "item {key}");
        }
    }
}
