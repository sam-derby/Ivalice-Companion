import { useState, type KeyboardEvent, type ReactNode } from 'react';
import type { ValueState } from './ipc';
import type {
  CatalogueRef,
  Fact,
  ReaderDocument,
  ReaderIdentity,
  ReaderUnit,
} from './reader-ipc';
import {
  itemArt,
  jobArt,
  portraitArt,
  useReaderArt,
  type ReaderArt,
} from './reader-art';

export function ArtImage({
  src,
  label,
  variant,
  monster = false,
  jobAvatar = false,
}: {
  src: string | null;
  label: string;
  variant: 'portrait' | 'job' | 'item' | 'sprite';
  monster?: boolean;
  jobAvatar?: boolean;
}) {
  const [failed, setFailed] = useState(false);
  return src && !failed ? (
    <img
      className={`reader-art reader-art-${variant}${jobAvatar ? ' reader-art-job-avatar' : ''}`}
      src={src}
      alt={label}
      loading="lazy"
      onError={() => {
        setFailed(true);
      }}
    />
  ) : (
    <span
      role="img"
      className={`reader-art reader-art-${variant} reader-art-fallback${monster ? ' reader-art-monster' : ''}`}
      aria-label={
        variant === 'portrait'
          ? `${label} represented by a ${monster ? 'monster' : 'neutral'} silhouette`
          : `${label} unavailable`
      }
    >
      {variant === 'portrait' &&
        (monster ? (
          <svg viewBox="0 0 64 64" aria-hidden="true" focusable="false">
            <path d="M15 20 7 6l19 9h12L57 6l-8 14 4 13-6 20H17l-6-20z" />
            <path d="M22 33h6m8 0h6M26 44l6 4 6-4" />
          </svg>
        ) : (
          <svg viewBox="0 0 64 64" aria-hidden="true" focusable="false">
            <circle cx="32" cy="23" r="12" />
            <path d="M11 58c2-13 9-20 21-20s19 7 21 20" />
          </svg>
        ))}
    </span>
  );
}

function isMonster(unit: ReaderUnit): boolean {
  return (
    unit.kind.value.state === 'known' && unit.kind.value.value === 'monster'
  );
}

function unitPortrait(art: ReaderArt | null, unit: ReaderUnit): string | null {
  const identity = unit.portrait.value;
  return identity.state === 'known' ? portraitArt(art, identity.value) : null;
}

function unitSex(unit: ReaderUnit): 'male' | 'female' | null {
  const sex = unit.stored.sex.value;
  if (sex.state !== 'known') return null;
  return sex.value === 'Male'
    ? 'male'
    : sex.value === 'Female'
      ? 'female'
      : null;
}

export function rosterArt(
  art: ReaderArt | null,
  unit: ReaderUnit,
): { src: string | null; label: string; jobAvatar: boolean } {
  if (isMonster(unit)) {
    return {
      src: null,
      label: `${unitName(unit)} portrait`,
      jobAvatar: false,
    };
  }
  const portrait = unitPortrait(art, unit);
  if (portrait) {
    return {
      src: portrait,
      label: `${unitName(unit)} portrait`,
      jobAvatar: false,
    };
  }
  const job = unit.current_job.value;
  const illustration =
    !isMonster(unit) && job.state === 'known'
      ? jobArt(art, job.value.id, unitSex(unit))
      : null;
  return illustration
    ? {
        src: illustration,
        label: `${unitName(unit)} ${jobName(unit)} job illustration`,
        jobAvatar: true,
      }
    : { src: null, label: `${unitName(unit)} portrait`, jobAvatar: false };
}

export function readerIdentityKey(identity: ReaderIdentity): string {
  return JSON.stringify([
    identity.session,
    identity.snapshot_generation,
    identity.resource_generation,
    identity.resource_token,
    identity.manual_slot,
  ]);
}

function visible<T>(fact: Fact<T>): ValueState<T> {
  return fact.value;
}

function valueLabel<T>(
  value: ValueState<T>,
  format: (value: T) => string,
): string {
  switch (value.state) {
    case 'known':
      return format(value.value);
    case 'unknown':
      return 'Unknown';
    case 'absent':
      return 'None';
    case 'unsupported':
      return 'Unsupported';
  }
}

function overviewLabel<T>(
  value: ValueState<T>,
  format: (value: T) => string,
): string {
  if (value.state === 'unknown') return 'Not available yet';
  if (value.state === 'unsupported') return 'Unavailable';
  return valueLabel(value, format);
}

export function unitName(unit: ReaderUnit): string {
  return valueLabel(visible(unit.name), (name) => name.text);
}

export function jobName(unit: ReaderUnit): string {
  return valueLabel(visible(unit.current_job), (job) =>
    valueLabel(job.label, (label) => label),
  );
}

function unitTags(unit: ReaderUnit): string[] {
  const tags: string[] = [];
  const membership = unit.membership.value;
  if (membership.state === 'known') {
    if (membership.value === 'guest') tags.push('Guest');
    if (membership.value === 'inactive') tags.push('Inactive');
  }
  const kind = unit.kind.value;
  if (kind.state === 'known' && kind.value === 'monster') tags.push('Monster');
  return tags;
}

function CatalogueValue({
  fact,
  art,
  emptyLabel = 'None',
  unknownLabel,
}: {
  fact: Fact<CatalogueRef>;
  art?: ReaderArt | null;
  emptyLabel?: string;
  unknownLabel?: string;
}) {
  const value = visible(fact);
  return value.state === 'known' ? (
    <span className="catalogue-value">
      {art !== undefined && value.value.id.startsWith('item:') && (
        <ArtImage
          src={referenceArt(value.value, art)}
          label={`${valueLabel(value.value.label, String)} icon`}
          variant="item"
        />
      )}
      {valueLabel(value.value.label, (label) => label)}
    </span>
  ) : value.state === 'absent' ? (
    emptyLabel
  ) : value.state === 'unknown' && unknownLabel ? (
    unknownLabel
  ) : (
    valueLabel(value, () => '')
  );
}

function referenceArt(
  reference: CatalogueRef,
  art: ReaderArt | null,
): string | null {
  return reference.id.startsWith('item:') ? itemArt(art, reference.id) : null;
}

export function CatalogueDescription({
  fact,
  label,
}: {
  fact: Fact<CatalogueRef>;
  label: string;
}) {
  const value = visible(fact);
  return value.state === 'known' &&
    value.value.label.state === 'known' &&
    value.value.description.state === 'known' ? (
    <p className="job-description" aria-label={label}>
      {value.value.description.value.replace(/<\/?color(?:=\d+)?>/g, '')}
    </p>
  ) : null;
}

function AbilityRow({
  label,
  fact,
  unknownLabel,
}: {
  label: string;
  fact: Fact<CatalogueRef>;
  unknownLabel?: string;
}) {
  return (
    <div className="ability-detail">
      <dt>{label}</dt>
      <dd>
        <CatalogueValue
          fact={fact}
          {...(unknownLabel === undefined ? {} : { unknownLabel })}
        />
      </dd>
      <CatalogueDescription fact={fact} label={`${label} description`} />
    </div>
  );
}

function JobsDetails({
  unit,
  art,
}: {
  unit: ReaderUnit;
  art: ReaderArt | null;
}) {
  const jobs = visible(unit.jobs);
  const current = unit.current_job.value;
  const progress =
    jobs.state === 'known'
      ? jobs.value.filter(
          (entry) =>
            [
              entry.level.value,
              entry.current_jp.value,
              entry.total_jp.value,
            ].some((value) => value.state === 'known' && value.value > 0) ||
            (current.state === 'known' &&
              entry.job.value.state === 'known' &&
              entry.job.value.value.id === current.value.id),
        )
      : [];
  return jobs.state !== 'known' ? (
    <p>Job progress: {valueLabel(jobs, () => '')}</p>
  ) : progress.length === 0 ? (
    <p>No job progress yet.</p>
  ) : (
    <div className="job-progress-grid">
      {progress.map((entry) => {
        const job = entry.job.value;
        const name =
          job.state === 'known'
            ? valueLabel(job.value.label, String)
            : 'Job name unavailable';
        return (
          <div key={entry.slot} className="job-progress-card">
            <div className="job-progress-heading">
              <ArtImage
                src={
                  job.state === 'known'
                    ? jobArt(art, job.value.id, unitSex(unit))
                    : null
                }
                label={`${name} job illustration`}
                variant="job"
              />
              <h4>{name}</h4>
            </div>
            <dl className="details" aria-label={`${name} progress`}>
              <div>
                <dt>Job level</dt>
                <dd>{valueLabel(visible(entry.level), String)}</dd>
              </div>
              <div>
                <dt>Spendable JP</dt>
                <dd>{valueLabel(visible(entry.current_jp), String)}</dd>
              </div>
              <div>
                <dt>Job EXP</dt>
                <dd>{valueLabel(visible(entry.total_jp), String)}</dd>
              </div>
            </dl>
          </div>
        );
      })}
    </div>
  );
}

function AbilitiesDetails({ unit }: { unit: ReaderUnit }) {
  return (
    <>
      <p>
        Saved equipped abilities. These selections do not establish learned
        state or loadout legality.
      </p>
      <dl className="details ability-details">
        <AbilityRow label="Reaction" fact={unit.abilities.reaction} />
        <AbilityRow label="Support" fact={unit.abilities.support} />
        <AbilityRow label="Movement" fact={unit.abilities.movement} />
        <AbilityRow
          label="Primary command"
          fact={unit.abilities.primary_command}
          unknownLabel="Command not available yet"
        />
        <AbilityRow
          label="Secondary command"
          fact={unit.abilities.secondary_command}
        />
      </dl>
    </>
  );
}

const equipmentSlots = [
  ['right', 'Right arm'],
  ['left', 'Left arm'],
  ['head', 'Head'],
  ['body', 'Body'],
  ['accessory', 'Accessory'],
] as const;

export function armEquipment(unit: ReaderUnit, side: 'right' | 'left') {
  const weapon = unit.equipment[`${side}_weapon`];
  const shield = unit.equipment[`${side}_shield`];
  if (weapon.value.state === 'known') return weapon;
  if (shield.value.state === 'known') return shield;
  if (weapon.value.state === 'unknown' || shield.value.state === 'unknown')
    return weapon.value.state === 'unknown' ? weapon : shield;
  if (
    weapon.value.state === 'unsupported' ||
    shield.value.state === 'unsupported'
  )
    return weapon.value.state === 'unsupported' ? weapon : shield;
  return weapon;
}

export function secondArmItem(unit: ReaderUnit, side: 'right' | 'left') {
  const weapon = unit.equipment[`${side}_weapon`];
  const shield = unit.equipment[`${side}_shield`];
  return weapon.value.state === 'known' && shield.value.state === 'known'
    ? shield
    : null;
}

export function EquipmentDetails({
  unit,
  art,
  compact = false,
  itemDetails,
}: {
  unit: ReaderUnit;
  art: ReaderArt | null;
  compact?: boolean;
  itemDetails?: Record<string, string[]>;
}) {
  return (
    <>
      {!compact && <p>Saved equipment slots.</p>}
      <>
        <dl className="details equipment-details">
          {equipmentSlots.map(([key, label]) => (
            <div key={key} className="equipment-detail">
              <dt>{label}</dt>
              <dd>
                <CatalogueValue
                  fact={
                    key === 'right' || key === 'left'
                      ? armEquipment(unit, key)
                      : unit.equipment[key]
                  }
                  art={art}
                  emptyLabel="Empty"
                />
                {(key === 'right' || key === 'left') &&
                  secondArmItem(unit, key) && (
                    <>
                      {' / '}
                      <CatalogueValue
                        fact={unit.equipment[`${key}_shield`]}
                        art={art}
                      />
                    </>
                  )}
              </dd>
              {[
                key === 'right' || key === 'left'
                  ? armEquipment(unit, key)
                  : unit.equipment[key],
                ...(key === 'right' || key === 'left'
                  ? [secondArmItem(unit, key)]
                  : []),
              ].map(
                (item, index) =>
                  item?.value.state === 'known' && (
                    <ul
                      key={index}
                      className="workspace-item-effects"
                      aria-label={`${label} effects`}
                    >
                      {itemDetails?.[item.value.value.id]?.map((detail) => (
                        <li key={detail}>{detail}</li>
                      ))}
                    </ul>
                  ),
              )}
              {compact ? (
                [
                  key === 'right' || key === 'left'
                    ? armEquipment(unit, key)
                    : unit.equipment[key],
                  ...(key === 'right' || key === 'left'
                    ? [secondArmItem(unit, key)]
                    : []),
                ].some(
                  (item) =>
                    item?.value.state === 'known' &&
                    item.value.value.description.state === 'known' &&
                    item.value.value.description.value.trim() !== '',
                ) ? (
                  <details className="workspace-equipment-description">
                    <summary>Description</summary>
                    <CatalogueDescription
                      fact={
                        key === 'right' || key === 'left'
                          ? armEquipment(unit, key)
                          : unit.equipment[key]
                      }
                      label={`${label} description`}
                    />
                    {(key === 'right' || key === 'left') &&
                      secondArmItem(unit, key) && (
                        <CatalogueDescription
                          fact={unit.equipment[`${key}_shield`]}
                          label={`${label} shield description`}
                        />
                      )}
                  </details>
                ) : null
              ) : (
                <>
                  <CatalogueDescription
                    fact={
                      key === 'right' || key === 'left'
                        ? armEquipment(unit, key)
                        : unit.equipment[key]
                    }
                    label={`${label} description`}
                  />
                  {(key === 'right' || key === 'left') &&
                    secondArmItem(unit, key) && (
                      <CatalogueDescription
                        fact={unit.equipment[`${key}_shield`]}
                        label={`${label} shield description`}
                      />
                    )}
                </>
              )}
            </div>
          ))}
        </dl>
        {!compact && (
          <p>
            Saved equipment sets:{' '}
            {valueLabel(unit.combat_sets.value, (sets) => String(sets.length))}
          </p>
        )}
      </>
    </>
  );
}

export function CombatSetsDetails({
  unit,
  renderAbilities,
}: {
  unit: ReaderUnit;
  renderAbilities?: (set: number) => ReactNode;
}) {
  const sets = visible(unit.combat_sets);
  if (sets.state !== 'known') {
    return <p>Saved combat sets: {valueLabel(sets, () => '')}</p>;
  }
  const assigned = renderAbilities
    ? sets.value
    : sets.value.filter(
        (set) =>
          set.assigned.value.state !== 'known' || set.assigned.value.value,
      );
  if (assigned.length === 0) return <p>Unassigned</p>;
  return (
    <>
      {assigned.map((set) => {
        const abilityFields = renderAbilities?.(set.key);
        return (
          <details
            key={set.key}
            className="combat-set"
            open={
              set.key ===
              (unit.selected_combat_set.value.state === 'known'
                ? unit.selected_combat_set.value.value
                : 0)
            }
          >
            <summary>
              Set {set.key + 1}
              {set.name.value.state === 'known' &&
              set.name.value.value.trim() !== ''
                ? `: ${set.name.value.value}`
                : ''}
              {set.assigned.value.state === 'known' && !set.assigned.value.value
                ? ' · Unassigned'
                : ''}
              {unit.selected_combat_set.value.state === 'known' &&
              unit.selected_combat_set.value.value === set.key
                ? ' · Active'
                : ''}
            </summary>
            <dl className="details">
              <div>
                <dt>Job</dt>
                <dd>
                  <CatalogueValue fact={set.job} />
                </dd>
              </div>
              <div>
                <dt>Right hand</dt>
                <dd>
                  <CatalogueValue fact={set.right_hand} emptyLabel="Empty" />
                </dd>
              </div>
              <div>
                <dt>Left hand</dt>
                <dd>
                  <CatalogueValue fact={set.left_hand} emptyLabel="Empty" />
                </dd>
              </div>
              <div>
                <dt>Head</dt>
                <dd>
                  <CatalogueValue fact={set.head} emptyLabel="Empty" />
                </dd>
              </div>
              <div>
                <dt>Body</dt>
                <dd>
                  <CatalogueValue fact={set.body} emptyLabel="Empty" />
                </dd>
              </div>
              <div>
                <dt>Accessory</dt>
                <dd>
                  <CatalogueValue fact={set.accessory} emptyLabel="Empty" />
                </dd>
              </div>
              <div>
                <dt>Primary command</dt>
                <dd>
                  <CatalogueValue fact={set.abilities.primary_command} />
                </dd>
              </div>
              {!abilityFields && (
                <>
                  <div>
                    <dt>Secondary command</dt>
                    <dd>
                      <CatalogueValue fact={set.abilities.secondary_command} />
                    </dd>
                  </div>
                  <div>
                    <dt>Reaction</dt>
                    <dd>
                      <CatalogueValue fact={set.abilities.reaction} />
                    </dd>
                  </div>
                  <div>
                    <dt>Support</dt>
                    <dd>
                      <CatalogueValue fact={set.abilities.support} />
                    </dd>
                  </div>
                  <div>
                    <dt>Movement</dt>
                    <dd>
                      <CatalogueValue fact={set.abilities.movement} />
                    </dd>
                  </div>
                </>
              )}
              <div>
                <dt>Double hand</dt>
                <dd>
                  {valueLabel(set.double_hand.value, (value) =>
                    value ? 'Yes' : 'No',
                  )}
                </dd>
              </div>
            </dl>
            {abilityFields}
          </details>
        );
      })}
    </>
  );
}

const detailTabs = [
  'Overview',
  'Jobs',
  'Abilities',
  'Sets',
  'Equipment',
] as const;
type DetailTab = (typeof detailTabs)[number];

export function EvasionDetails({ unit }: { unit: ReaderUnit }) {
  return (
    <>
      {unit.effective.evasion.value.state === 'known' ? (
        <dl className="details" aria-label="Evasion by source">
          {unit.effective.evasion.value.value.map((entry) => (
            <div key={entry.source}>
              <dt>
                {entry.source.charAt(0).toUpperCase() +
                  entry.source.slice(1).replace('_', ' ')}
              </dt>
              <dd>
                Physical{' '}
                {valueLabel(
                  visible(entry.physical_basis_points),
                  (value) => `${String(value / 100)}%`,
                )}
                {' · '}Magical{' '}
                {valueLabel(
                  visible(entry.magical_basis_points),
                  (value) => `${String(value / 100)}%`,
                )}
              </dd>
            </div>
          ))}
        </dl>
      ) : (
        <p>{valueLabel(visible(unit.effective.evasion), String)}</p>
      )}
    </>
  );
}

export function UnitDetails({
  unit,
  art,
}: {
  unit: ReaderUnit;
  art: ReaderArt | null;
}) {
  const [tab, setTab] = useState<DetailTab>('Overview');
  const job = visible(unit.current_job);
  return (
    <section className="unit-detail" aria-labelledby="reader-unit-heading">
      <h3 id="reader-unit-heading">{unitName(unit)}</h3>
      <div className="unit-artwork">
        <figure>
          <ArtImage
            src={unitPortrait(art, unit)}
            label={`${unitName(unit)} portrait`}
            variant="portrait"
            monster={isMonster(unit)}
          />
        </figure>
        {job.state === 'known' && (
          <figure>
            <ArtImage
              src={jobArt(art, job.value.id, unitSex(unit))}
              label={`${jobName(unit)} job illustration`}
              variant="job"
            />
          </figure>
        )}
      </div>
      <div className="unit-tabs" role="tablist" aria-label="Unit detail tabs">
        {detailTabs.map((label, index) => (
          <button
            key={label}
            type="button"
            role="tab"
            id={`reader-tab-${label.toLowerCase()}`}
            aria-controls={`reader-panel-${label.toLowerCase()}`}
            aria-selected={tab === label}
            tabIndex={tab === label ? 0 : -1}
            onClick={() => {
              setTab(label);
            }}
            onKeyDown={(event) => {
              let next: number;
              switch (event.key) {
                case 'ArrowRight':
                  next = (index + 1) % detailTabs.length;
                  break;
                case 'ArrowLeft':
                  next = (index + detailTabs.length - 1) % detailTabs.length;
                  break;
                case 'Home':
                  next = 0;
                  break;
                case 'End':
                  next = detailTabs.length - 1;
                  break;
                default:
                  return;
              }
              event.preventDefault();
              const nextTab = detailTabs[next];
              if (nextTab) {
                setTab(nextTab);
                const buttons = event.currentTarget
                  .closest('[role="tablist"]')
                  ?.querySelectorAll('button');
                buttons?.[next]?.focus();
              }
            }}
          >
            {label}
          </button>
        ))}
      </div>
      <div
        role="tabpanel"
        id={`reader-panel-${tab.toLowerCase()}`}
        aria-labelledby={`reader-tab-${tab.toLowerCase()}`}
        tabIndex={0}
      >
        {tab === 'Overview' ? (
          <>
            <dl className="details">
              <div>
                <dt>Name</dt>
                <dd>{unitName(unit)}</dd>
              </div>
              <div>
                <dt>Current job</dt>
                <dd>{jobName(unit)}</dd>
              </div>
              <div>
                <dt>Level</dt>
                <dd>{valueLabel(visible(unit.stored.level), String)}</dd>
              </div>
              {(
                [
                  ['XP', unit.stored.experience],
                  ['Bravery', unit.stored.brave],
                  ['Faith', unit.stored.faith],
                ] as const
              ).map(([label, fact]) => (
                <div key={label}>
                  <dt>{label}</dt>
                  <dd>{valueLabel(visible(fact), String)}</dd>
                </div>
              ))}
              <div>
                <dt>Sex</dt>
                <dd>{valueLabel(unit.stored.sex.value, String)}</dd>
              </div>
              <div>
                <dt>Zodiac</dt>
                <dd>{valueLabel(unit.stored.zodiac.value, String)}</dd>
              </div>
            </dl>
            {job.state === 'known' &&
              job.value.description.state === 'known' && (
                <details className="optional-unit-facts">
                  <summary>About {jobName(unit)}</summary>
                  <p className="job-description">
                    {job.value.description.value}
                  </p>
                </details>
              )}
            <h3>Displayed combat stats</h3>
            <dl
              className="details effective-stats"
              aria-label="Displayed combat stats"
            >
              {(
                [
                  ['HP', unit.effective.hp],
                  ['MP', unit.effective.mp],
                  ['PA', unit.effective.physical_attack],
                  ['MA', unit.effective.magical_attack],
                  ['Speed', unit.effective.speed],
                  ['Move', unit.effective.movement_tiles],
                  ['Jump', unit.effective.jump_tiles],
                ] as const
              ).map(([label, fact]) => (
                <div key={label}>
                  <dt>{label}</dt>
                  <dd>{valueLabel(visible(fact), String)}</dd>
                </div>
              ))}
            </dl>
            <h3>Evasion</h3>
            <EvasionDetails unit={unit} />
          </>
        ) : tab === 'Jobs' ? (
          <JobsDetails unit={unit} art={art} />
        ) : tab === 'Abilities' ? (
          <AbilitiesDetails unit={unit} />
        ) : tab === 'Sets' ? (
          <CombatSetsDetails unit={unit} />
        ) : (
          <EquipmentDetails unit={unit} art={art} />
        )}
      </div>
    </section>
  );
}

// The accepted producer currently supports positive signed-32-bit Unix seconds.
const MAX_SUPPORTED_SAVED_AT_SECONDS = 2_147_483_647;

function SavedAt({ fact }: { fact: Fact<number> }) {
  const value = visible(fact);
  if (value.state !== 'known') return valueLabel(value, String);
  if (
    !Number.isSafeInteger(value.value) ||
    value.value <= 0 ||
    value.value > MAX_SUPPORTED_SAVED_AT_SECONDS
  ) {
    return 'Unknown';
  }
  const iso = new Date(value.value * 1000).toISOString();
  return (
    <time id="reader-saved-at" dateTime={iso}>
      {iso.slice(0, 19).replace('T', ' ')} UTC
    </time>
  );
}

/** App keys this component by the complete snapshot/resource identity. */
export function ReaderRoster({ reader }: { reader: ReaderDocument }) {
  const art = useReaderArt();
  const [section, setSection] = useState<'party' | 'inventory' | 'save'>(
    'party',
  );
  const [selectedKey, setSelectedKey] = useState<number | null>(null);
  const [inventoryQuery, setInventoryQuery] = useState('');
  const [inventoryCategory, setInventoryCategory] = useState('all');
  const roster = visible(reader.roster);
  const units = roster.state === 'known' ? roster.value : [];
  const inventory = visible(reader.inventory);
  const holdings = inventory.state === 'known' ? inventory.value : [];
  const owned = holdings.filter((holding) =>
    holding.quantity.value.state === 'known'
      ? holding.quantity.value.value > 0
      : true,
  );
  const categories = [
    ...new Set(
      owned.flatMap((holding) =>
        holding.category.value.state === 'known'
          ? [holding.category.value.value]
          : [],
      ),
    ),
  ].sort((left, right) => left.localeCompare(right));
  const query = inventoryQuery.trim().toLocaleLowerCase();
  const visibleHoldings = owned.filter((holding) => {
    const category = holding.category.value;
    if (inventoryCategory === 'unknown' && category.state === 'known')
      return false;
    if (
      inventoryCategory.startsWith('category:') &&
      (category.state !== 'known' ||
        category.value !== inventoryCategory.slice(9))
    )
      return false;
    if (query.length === 0) return true;
    const item = holding.item.value;
    const name =
      item.state === 'known' && item.value.label.state === 'known'
        ? item.value.label.value
        : '';
    return (
      name.toLocaleLowerCase().includes(query) ||
      `storage position ${String(holding.key)}`.includes(query)
    );
  });
  const selected = units.find((unit) => unit.key === selectedKey);

  function moveFocus(event: KeyboardEvent<HTMLButtonElement>, index: number) {
    let next: number;
    switch (event.key) {
      case 'ArrowDown':
        next = Math.min(index + 1, units.length - 1);
        break;
      case 'ArrowUp':
        next = Math.max(index - 1, 0);
        break;
      case 'Home':
        next = 0;
        break;
      case 'End':
        next = units.length - 1;
        break;
      default:
        return;
    }
    event.preventDefault();
    const nextUnit = units[next];
    const buttons = event.currentTarget
      .closest('ol')
      ?.querySelectorAll('button');
    const nextButton = buttons?.[next];
    if (nextUnit && nextButton) {
      setSelectedKey(nextUnit.key);
      nextButton.focus();
    }
  }

  return (
    <>
      <nav className="reader-section-tabs" aria-label="Reader sections">
        {(
          [
            ['party', 'Party'],
            ['inventory', 'Inventory'],
            ['save', 'Save overview'],
          ] as const
        ).map(([key, label]) => (
          <button
            key={key}
            type="button"
            aria-pressed={section === key}
            onClick={() => {
              setSection(key);
            }}
          >
            {label}
          </button>
        ))}
      </nav>
      {section === 'save' && (
        <section
          className="panel save-overview"
          aria-labelledby="reader-save-heading"
        >
          <h2 id="reader-save-heading">Save overview</h2>
          <dl className="details overview-facts">
            <div>
              <dt>Chapter</dt>
              <dd>{overviewLabel(reader.progress.chapter.value, String)}</dd>
            </div>
            <div>
              <dt>Current area</dt>
              <dd>
                {overviewLabel(reader.progress.location.value, (area) =>
                  overviewLabel(area.label, String),
                )}
              </dd>
            </div>
            <div>
              <dt>Ramza level</dt>
              <dd>
                {overviewLabel(reader.progress.ramza_level.value, String)}
              </dd>
            </div>
          </dl>
          <details className="save-development-details">
            <summary>More save data</summary>
            <dl className="details">
              <div>
                <dt>Slot title</dt>
                <dd>{valueLabel(reader.progress.title.value, String)}</dd>
              </div>
              <div>
                <dt>Saved at (UTC)</dt>
                <dd>
                  <SavedAt fact={reader.progress.saved_at_unix_seconds} />
                </dd>
              </div>
              <div>
                <dt>Saved hero name</dt>
                <dd>{valueLabel(reader.progress.hero_name.value, String)}</dd>
              </div>
              <div>
                <dt>Next event ID</dt>
                <dd>
                  {valueLabel(reader.progress.next_event_id.value, String)}
                </dd>
              </div>
              <div>
                <dt>Unnamed event values set</dt>
                <dd>
                  {valueLabel(
                    reader.progress.unnamed_event_values.value,
                    String,
                  )}
                </dd>
              </div>
              <div>
                <dt>Gil</dt>
                <dd>{valueLabel(reader.gil.value, String)}</dd>
              </div>
            </dl>
          </details>
        </section>
      )}
      {section === 'party' && (
        <section className="panel" aria-labelledby="reader-roster-heading">
          <h2 id="reader-roster-heading">Party roster</h2>
          {roster.state !== 'known' ? (
            <p>Roster: {valueLabel(roster, () => '')}</p>
          ) : units.length === 0 ? (
            <p>No supported party members were reported for this slot.</p>
          ) : (
            <div className="reader-columns">
              <div>
                <p className="roster-instructions">
                  Select a member. Use Tab or the arrow keys to move through the
                  roster.
                </p>
                <ol className="reader-roster" aria-label="Party members">
                  {units.map((unit, index) => (
                    <li key={unit.key}>
                      <button
                        type="button"
                        aria-label={`Member ${String(index + 1)} ${unitName(unit)} ${jobName(unit)} Level ${valueLabel(visible(unit.stored.level), String)} ${unitTags(unit).join(' ')}`.trim()}
                        aria-pressed={selectedKey === unit.key}
                        data-unit-key={unit.key}
                        aria-controls="reader-unit-detail"
                        onClick={() => {
                          setSelectedKey(unit.key);
                        }}
                        onKeyDown={(event) => {
                          moveFocus(event, index);
                        }}
                      >
                        <ArtImage
                          src={rosterArt(art, unit).src}
                          label={rosterArt(art, unit).label}
                          variant="portrait"
                          monster={isMonster(unit)}
                          jobAvatar={rosterArt(art, unit).jobAvatar}
                        />
                        <span className="member-position">
                          Member {index + 1}
                        </span>
                        <strong>{unitName(unit)}</strong>
                        <span className="member-job">{jobName(unit)}</span>
                        <span className="member-level">
                          Level {valueLabel(visible(unit.stored.level), String)}
                        </span>
                        {unitTags(unit).length > 0 && (
                          <span className="member-tags">
                            {unitTags(unit).join(' · ')}
                          </span>
                        )}
                      </button>
                    </li>
                  ))}
                </ol>
              </div>
              <div id="reader-unit-detail" aria-live="polite">
                {selected ? (
                  <UnitDetails
                    key={`${readerIdentityKey(reader.identity)}:${String(selected.key)}`}
                    unit={selected}
                    art={art}
                  />
                ) : (
                  <p>Select a party member to inspect their saved details.</p>
                )}
              </div>
            </div>
          )}
        </section>
      )}
      {section === 'inventory' && (
        <section className="panel" aria-labelledby="reader-inventory-heading">
          <h2 id="reader-inventory-heading">Party inventory</h2>
          {inventory.state !== 'known' ? (
            <p>Inventory: {valueLabel(inventory, () => '')}</p>
          ) : (
            <>
              <p>{owned.length} inventory entries with saved quantities.</p>
              <div className="inventory-controls">
                <label htmlFor="inventory-search">
                  Search party inventory
                  <input
                    id="inventory-search"
                    type="search"
                    value={inventoryQuery}
                    onChange={(event) => {
                      setInventoryQuery(event.target.value);
                    }}
                    placeholder="Item name or position"
                  />
                </label>
                <label htmlFor="inventory-category">
                  Category
                  <select
                    id="inventory-category"
                    value={inventoryCategory}
                    onChange={(event) => {
                      setInventoryCategory(event.target.value);
                    }}
                  >
                    <option value="all">All categories</option>
                    {owned.some(
                      (holding) => holding.category.value.state !== 'known',
                    ) && <option value="unknown">Unknown category</option>}
                    {categories.map((category) => (
                      <option key={category} value={`category:${category}`}>
                        {category}
                      </option>
                    ))}
                  </select>
                </label>
              </div>
              {owned.length === 0 ? (
                <p>No owned items recorded.</p>
              ) : visibleHoldings.length === 0 ? (
                <p>No matching holdings.</p>
              ) : (
                <ul className="inventory-list">
                  {visibleHoldings.map((holding) => (
                    <li key={holding.key}>
                      <span className="inventory-item-name">
                        <CatalogueValue fact={holding.item} art={art} />
                        {visible(holding.item).state !== 'known' && (
                          <> (position {holding.key})</>
                        )}
                      </span>
                      <span className="inventory-quantity">
                        &times; {valueLabel(holding.quantity.value, String)}
                      </span>
                    </li>
                  ))}
                </ul>
              )}
            </>
          )}
        </section>
      )}
    </>
  );
}
