//! TICSaveEditor.Core/Sections/BattleSection.cs and Records/PartyInventory.cs
//! at 07ea857: adjacent party, shop and found-item count regions.

use super::{ManualParseError, BATTLE_OFFSET, BATTLE_SIZE, UNIT_COUNT, UNIT_SIZE};

pub const PARTY_CAPACITY: usize = 0x105;
pub const SHOP_CAPACITY: usize = 0x105;
pub const FOUND_CAPACITY: usize = 0x80;
const COUNTS_OFFSET: usize = BATTLE_OFFSET + UNIT_COUNT * UNIT_SIZE;

pub(super) const fn party_counts_offset() -> usize {
    COUNTS_OFFSET
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BattleStores {
    pub party: [u8; PARTY_CAPACITY],
    pub shop: [u8; SHOP_CAPACITY],
    pub found: [u8; FOUND_CAPACITY],
}

impl BattleStores {
    pub(super) fn parse(slot: &[u8]) -> Result<Self, ManualParseError> {
        let end = BATTLE_OFFSET
            .checked_add(BATTLE_SIZE)
            .ok_or(ManualParseError::BattleBounds)?;
        let battle = slot
            .get(BATTLE_OFFSET..end)
            .ok_or(ManualParseError::BattleBounds)?;
        let start = COUNTS_OFFSET - BATTLE_OFFSET;
        let party_end = start + PARTY_CAPACITY;
        let shop_end = party_end + SHOP_CAPACITY;
        let found_end = shop_end + FOUND_CAPACITY;
        let party = battle
            .get(start..party_end)
            .ok_or(ManualParseError::BattleBounds)?
            .try_into()
            .map_err(|_| ManualParseError::BattleBounds)?;
        let shop = battle
            .get(party_end..shop_end)
            .ok_or(ManualParseError::BattleBounds)?
            .try_into()
            .map_err(|_| ManualParseError::BattleBounds)?;
        let found = battle
            .get(shop_end..found_end)
            .ok_or(ManualParseError::BattleBounds)?
            .try_into()
            .map_err(|_| ManualParseError::BattleBounds)?;
        Ok(Self { party, shop, found })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundaries_zero_maximum_and_store_separation() {
        let mut slot = vec![0; BATTLE_OFFSET + BATTLE_SIZE];
        slot[COUNTS_OFFSET] = 255;
        slot[COUNTS_OFFSET + PARTY_CAPACITY - 1] = 1;
        slot[COUNTS_OFFSET + PARTY_CAPACITY] = 2;
        slot[COUNTS_OFFSET + PARTY_CAPACITY + SHOP_CAPACITY - 1] = 3;
        slot[COUNTS_OFFSET + PARTY_CAPACITY + SHOP_CAPACITY] = 4;
        slot[COUNTS_OFFSET + PARTY_CAPACITY + SHOP_CAPACITY + FOUND_CAPACITY - 1] = 5;
        let before = slot.clone();
        let stores = BattleStores::parse(&slot).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(stores.party.len(), 261);
        assert_eq!(stores.party[0], 255);
        assert_eq!(stores.party[1], 0);
        assert_eq!(stores.party[260], 1);
        assert_eq!(stores.shop[0], 2);
        assert_eq!(stores.shop[260], 3);
        assert_eq!(stores.found[0], 4);
        assert_eq!(stores.found[127], 5);
        assert_eq!(slot, before);
        assert_eq!(
            BattleStores::parse(&slot[..slot.len() - 1]),
            Err(ManualParseError::BattleBounds)
        );
    }
}
