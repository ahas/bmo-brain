import type { Brain, Memory } from "./brain-api";

// The review of the base's candidates (docs/design.md, 「Raising a memory to the base」 and 「The screen」): the
// base found among the brains, each candidate's source read off its labels, and the reasons a decision moves a row
// with.

// The keys the base is filed under, an org brain's (docs/design.md, 「The base」).
export const BASE_KEY = "platform";

// The reasons a decision moves a row with: a candidate kept in its source, and one dropped with its source.
export const KEPT_IN_SOURCE = "kept-in-source";
export const DROPPED = "dropped";

// Whether a brain is the base.
export function isBase(brain: Pick<Brain, "kind" | "accountKey" | "agentKey"> | undefined): boolean {
  return brain !== undefined && brain.kind === "org" && brain.accountKey === BASE_KEY && brain.agentKey === BASE_KEY;
}

// Where a candidate was raised from: its source brain and memory, or none where its labels name neither.
export function sourceOf(memory: Pick<Memory, "labels">): { brainId: number; memoryId: number } | null {
  const brainId = Number(memory.labels?.sourceBrainId);
  const memoryId = Number(memory.labels?.sourceMemoryId);

  return Number.isInteger(brainId) && brainId > 0 && Number.isInteger(memoryId) && memoryId > 0 ? { brainId, memoryId } : null;
}

// The candidates among a brain's rows: the pending ones a source raised, oldest first.
export function candidatesOf(memories: Memory[]): Memory[] {
  return memories.filter((memory) => memory.status === "pending" && sourceOf(memory) !== null).toSorted((one, other) => one.id - other.id);
}
