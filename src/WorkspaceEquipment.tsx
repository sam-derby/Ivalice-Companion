import { useState } from 'react';
import type {
  ReaderUnit,
  Fact,
  CatalogueRef,
  EditContext,
  GearSlot,
  GearSource,
} from './reader-ipc';
import { ArtImage, armEquipment, secondArmItem } from './ReaderRoster';
import { itemArt, type ReaderArt } from './reader-art';
import { valueText } from './workspace-fields';
import { Tooltip } from './Tooltip';

export function description(fact: Fact<CatalogueRef>): string | undefined {
  return fact.value.state === 'known' &&
    fact.value.value.description.state === 'known'
    ? fact.value.value.description.value
    : undefined;
}

export function EquippedGear({
  unit,
  art,
  itemDetails,
  gearOptions,
  staged,
  held,
  onChange,
}: {
  unit: ReaderUnit;
  art: ReaderArt | null;
  itemDetails: Record<string, string[]>;
  gearOptions: NonNullable<EditContext['gearOptions']>[number] | undefined;
  staged?: Record<string, { itemKey: string; source: GearSource | null }>;
  held?: Record<number, number>;
  onChange:
    | ((slot: GearSlot, itemKey: string, source: GearSource | null) => void)
    | undefined;
}) {
  const [editing, setEditing] = useState<GearSlot | null>(null);
  function item(fact: Fact<CatalogueRef>) {
    const value = fact.value;
    const label =
      value.state === 'absent'
        ? 'Empty'
        : valueText(value, (ref) => valueText(ref.label));
    const details =
      value.state === 'known' ? (itemDetails[value.value.id] ?? []) : [];
    return (
      <Tooltip
        text={[label, description(fact), ...details].filter(Boolean).join('\n')}
      >
        {value.state === 'known' && (
          <ArtImage
            src={itemArt(art, value.value.id)}
            label={`${label} icon`}
            variant="item"
          />
        )}
        <span className="workspace-gear-name">{label}</span>
      </Tooltip>
    );
  }
  return (
    <dl className="details equipment-details">
      {(
        [
          ['right', 'Right arm'],
          ['left', 'Left arm'],
          ['head', 'Head'],
          ['body', 'Body'],
          ['accessory', 'Accessory'],
        ] as const
      ).map(([key, label]) => {
        const gearSlot: GearSlot =
          key === 'right' ? 'right_hand' : key === 'left' ? 'left_hand' : key;
        const slot = gearOptions?.slots.find(
          (entry) => entry.slot === gearSlot,
        );
        const stagedChoice = staged?.[gearSlot];
        const extra =
          key === 'right' || key === 'left' ? secondArmItem(unit, key) : null;
        return (
          <div className="equipment-detail" key={key}>
            <dt>{label}</dt>
            <dd>
              <div className="workspace-gear-line">
                {item(
                  key === 'right' || key === 'left'
                    ? armEquipment(unit, key)
                    : unit.equipment[key],
                )}
                {extra && item(extra)}
                {slot && onChange && (
                  <button
                    type="button"
                    aria-label={`Change ${label}`}
                    aria-expanded={editing === gearSlot}
                    onClick={() => {
                      setEditing(editing === gearSlot ? null : gearSlot);
                    }}
                  >
                    Change
                  </button>
                )}
              </div>
              {slot && stagedChoice && (
                <p className="workspace-gear-staged">
                  Staged:{' '}
                  {stagedChoice.itemKey === ''
                    ? 'Empty'
                    : (slot.choices.find(
                        (choice) => choice.key === stagedChoice.itemKey,
                      )?.label ?? stagedChoice.itemKey)}
                </p>
              )}
              {slot && editing === gearSlot && onChange && staged && held && (
                <GearEditor
                  slot={slot}
                  staged={staged}
                  held={held}
                  onChange={onChange}
                />
              )}
            </dd>
          </div>
        );
      })}
    </dl>
  );
}

function GearEditor({
  slot,
  staged,
  held,
  onChange,
}: {
  slot: NonNullable<EditContext['gearOptions']>[number]['slots'][number];
  staged: Record<string, { itemKey: string; source: GearSource | null }>;
  held: Record<number, number>;
  onChange: (
    slot: GearSlot,
    itemKey: string,
    source: GearSource | null,
  ) => void;
}) {
  const slotLabel =
    slot.slot === 'right_hand'
      ? 'Right arm'
      : slot.slot === 'left_hand'
        ? 'Left arm'
        : slot.slot.charAt(0).toUpperCase() + slot.slot.slice(1);
  const [search, setSearch] = useState<Record<string, string>>({});
  const [selected, setSelected] = useState<Record<string, string>>({});
  const selection = selected[slot.slot] ?? '';
  const current = staged[slot.slot]?.itemKey ?? slot.currentKey;
  const available = /^item:(\d+)$/.exec(selection);
  const quantity = available ? (held[Number(available[1])] ?? 0) : 0;
  const choices = slot.choices.filter((choice) =>
    choice.label
      .toLocaleLowerCase()
      .includes((search[slot.slot] ?? '').trim().toLocaleLowerCase()),
  );
  return (
    <div className="workspace-gear-inline-editor" key={slot.slot}>
      <label>
        Search compatible gear
        <input
          type="search"
          value={search[slot.slot] ?? ''}
          onChange={(event) => {
            setSearch((old) => ({
              ...old,
              [slot.slot]: event.target.value,
            }));
          }}
        />
      </label>
      <label>
        {slotLabel} item
        <select
          value={
            choices.some((choice) => choice.key === selection) ? selection : ''
          }
          onChange={(event) => {
            setSelected((old) => ({
              ...old,
              [slot.slot]: event.target.value,
            }));
          }}
        >
          <option value="">Choose an item</option>
          {choices.map((choice) => (
            <option value={choice.key} key={choice.key}>
              {choice.label}
            </option>
          ))}
        </select>
      </label>
      {selection && <p>Available held: {quantity}</p>}
      <div className="workspace-gear-actions">
        <button
          type="button"
          disabled={
            !selection ||
            selection === slot.currentKey ||
            quantity < 1 ||
            (staged[slot.slot]?.itemKey === selection &&
              staged[slot.slot]?.source === 'held')
          }
          onClick={() => {
            onChange(slot.slot, selection, 'held');
          }}
        >
          Equip from inventory
        </button>
        <button
          type="button"
          disabled={
            !selection ||
            selection === slot.currentKey ||
            quantity >= 99 ||
            (staged[slot.slot]?.itemKey === selection &&
              staged[slot.slot]?.source === 'create_and_equip')
          }
          onClick={() => {
            onChange(slot.slot, selection, 'create_and_equip');
          }}
        >
          Create and equip
        </button>
        <button
          type="button"
          disabled={current === ''}
          onClick={() => {
            onChange(slot.slot, '', null);
          }}
        >
          Unequip
        </button>
        {staged[slot.slot] && (
          <button
            type="button"
            onClick={() => {
              onChange(slot.slot, slot.currentKey, null);
            }}
          >
            Reset
          </button>
        )}
      </div>
      {selection && (
        <p className="workspace-gear-hint">
          Create and equip adds an equipped copy and one held copy.
        </p>
      )}
    </div>
  );
}
