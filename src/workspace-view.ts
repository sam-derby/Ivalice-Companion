export type WorkspaceSection =
  'game' | 'quests' | 'units' | 'inventory' | 'utilities';
export type UnitPanel = 'status' | 'equipment' | 'jobs';
export type AbilityKind = 'action' | 'reaction' | 'support' | 'movement';

export interface WorkspaceView {
  section: WorkspaceSection;
  panel: UnitPanel;
  selectedUnit: number | null;
  selectedJobs: Record<number, number>;
  abilityKind: AbilityKind;
  unitQuery: string;
  itemQuery: string;
  itemCategory: string;
  itemView: 'held' | 'all';
  selectedItem: number | null;
}
