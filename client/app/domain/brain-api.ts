import { relayAddress } from "bmo-ui/app/domain/relay";

import type { components } from "bmo-brain-client/schema";

// The brain's routes as the screen calls them (docs/boundary-api.md), each at the address the console's relay
// answers for the service. The screen reads one brain at a time, so every call but the brain list names it.

type Schemas = components["schemas"];

export type Brain = Schemas["Brain"];
export type Layer = Schemas["Layer"];
export type Memory = Schemas["Memory"];
export type MemoryStatus = Schemas["MemoryStatus"];
export type Subject = Schemas["Subject"];
export type SubjectKey = Schemas["SubjectKey"];
export type Tally = Schemas["Tally"];
export type TallyBy = Schemas["TallyBy"];
export type TypeCount = Schemas["TypeCount"];
export type Learning = Schemas["Learning"];
export type Learned = Schemas["Learned"];
export type MemoryVersion = Schemas["MemoryVersion"];
export type NewMemory = Schemas["NewMemory"];

interface Page<T> {
  items: T[];
  more: boolean;
}

const SERVICE = "brain";

// Who a write from the screen is recorded as: the screen does not know the reader's name, and the brain is
// told the screen wrote it.
export const SCREEN_ACTOR = "screen";

// The largest page a route answers.
const MOST = 500;

// The address of one of the brain's routes, complete as given.
export function brainAddress(path: string): string {
  return relayAddress(SERVICE, path);
}

// A query's values with the empty ones left out.
type Asked = Record<string, string | number | boolean | null | undefined>;

function asked(query: Asked): Record<string, string> {
  return Object.fromEntries(
    Object.entries(query)
      .filter(([, value]) => value !== null && value !== undefined && value !== "")
      .map(([name, value]) => [name, String(value)])
  );
}

function under(brainId: number, path: string): string {
  return brainAddress(`/brains/${brainId}${path}`);
}

// Every page of a list, read until the last.
async function whole<T extends { id: number }>(address: string, query: Asked = {}): Promise<T[]> {
  const items: T[] = [];
  let after: number | null = null;

  for (;;) {
    const page: Page<T> = await $fetch<Page<T>>(address, { query: asked({ ...query, after, limit: MOST }) });

    items.push(...page.items);

    if (!page.more || page.items.length === 0) return items;

    after = page.items[page.items.length - 1]!.id;
  }
}

export function brains(): Promise<Brain[]> {
  return whole<Brain>(brainAddress("/brains"));
}

// The words a list is narrowed by and the page it stands at: `after` the member the page read last ended on, the
// layer's or the subject's id, or a type's word.
interface PagedList {
  q?: string;
  after?: number | string | null;
  limit?: number;
  [name: string]: string | number | boolean | null | undefined;
}

export function layerPage(brainId: number, query: PagedList): Promise<Page<Layer>> {
  return $fetch<Page<Layer>>(under(brainId, "/layers"), { query: asked(query) });
}

export function layer(brainId: number, id: number): Promise<Layer> {
  return $fetch<Layer>(under(brainId, `/layers/${id}`));
}

export function subjectPage(brainId: number, query: PagedList & { ids?: string }): Promise<Page<Subject>> {
  return $fetch<Page<Subject>>(under(brainId, "/subjects"), { query: asked(query) });
}

export function subject(brainId: number, id: number): Promise<Subject> {
  return $fetch<Subject>(under(brainId, `/subjects/${id}`));
}

export function typePage(brainId: number, query: PagedList): Promise<Page<TypeCount>> {
  return $fetch<Page<TypeCount>>(under(brainId, "/types"), { query: asked(query) });
}

// The filters the memory list and the tallies share, as the routes take them.
export interface MemoryFilter {
  status?: string;
  type?: string;
  excludeTypes?: string;
  layerId?: number;
  unfiled?: boolean;
  subjectId?: number;
  key?: string;
  lineageId?: number;
  ids?: string;
  supersededBy?: string;
  label?: string;
  q?: string;
}

// The screen's lists are a person's view of the brain, never the agent's reading, so they record no use.
const UNRECORDED = { record: false };

export function memoryPage(
  brainId: number,
  filter: MemoryFilter,
  order: string,
  after: number | null,
  limit = 50
): Promise<Page<Memory>> {
  return $fetch<Page<Memory>>(under(brainId, "/memories"), {
    query: asked({ ...filter, ...UNRECORDED, order, after, limit })
  });
}

export function memoriesNamed(brainId: number, filter: MemoryFilter): Promise<Memory[]> {
  return whole<Memory>(under(brainId, "/memories"), { ...filter, ...UNRECORDED });
}

export function tallies(brainId: number, by: TallyBy, filter: MemoryFilter, groups?: string): Promise<Tally[]> {
  return $fetch<Tally[]>(under(brainId, "/tallies"), { query: asked({ ...filter, by, groups }) });
}

export function memory(brainId: number, id: number): Promise<Memory> {
  return $fetch<Memory>(under(brainId, `/memories/${id}`));
}

export function lineage(brainId: number, lineageId: number): Promise<Memory[]> {
  return $fetch<Memory[]>(under(brainId, `/lineages/${lineageId}`));
}

export function persona(brainId: number, layer: string): Promise<string> {
  return $fetch<string>(under(brainId, "/persona"), { query: { layers: layer }, responseType: "text" });
}

// A subject's sheet as the brain renders it from the facts about it; empty where it holds none.
export function sheet(brainId: number, subjectId: number): Promise<string> {
  return $fetch<string>(under(brainId, `/subjects/${subjectId}/sheet`), { responseType: "text" });
}

export function exportAddress(brainId: number, layer: string): string {
  return `${under(brainId, "/export")}?layers=${encodeURIComponent(layer)}`;
}

export function writeVersion(brainId: number, id: number, version: Omit<MemoryVersion, "actorName">): Promise<Memory> {
  return $fetch<Memory>(under(brainId, `/memories/${id}/versions`), {
    method: "POST",
    body: { ...version, actorName: SCREEN_ACTOR }
  });
}

export function moveMemory(
  brainId: number,
  id: number,
  status: MemoryStatus,
  reason: string | null,
  text: string | null
): Promise<Memory> {
  return $fetch<Memory>(under(brainId, `/memories/${id}`), {
    method: "PATCH",
    body: asked({ status, reason, text, actorName: SCREEN_ACTOR })
  });
}

export function restoreMemory(brainId: number, id: number, text: string | null): Promise<Memory> {
  return $fetch<Memory>(under(brainId, `/memories/${id}/restorations`), {
    method: "POST",
    body: asked({ text, actorName: SCREEN_ACTOR })
  });
}

export function confirmMemory(brainId: number, id: number): Promise<unknown> {
  return $fetch(under(brainId, "/confirmations"), {
    method: "POST",
    body: { memoryIds: [id], actorName: SCREEN_ACTOR }
  });
}

export function writeMemory(brainId: number, memory: Omit<NewMemory, "actorName">): Promise<Memory> {
  return $fetch<Memory>(under(brainId, "/memories"), {
    method: "POST",
    body: { ...memory, actorName: SCREEN_ACTOR }
  });
}

export function similar(brainId: number, text: string, subjects: SubjectKey[]): Promise<Memory[]> {
  return $fetch<Memory[]>(under(brainId, "/similar"), { method: "POST", body: { text, subjects, limit: 10 } });
}

export function learn(brainId: number, learning: Omit<Learning, "actorName" | "source">): Promise<Learned> {
  return $fetch<Learned>(under(brainId, "/learnings"), {
    method: "POST",
    body: { ...learning, source: "owner", actorName: SCREEN_ACTOR }
  });
}

export function addLayer(brainId: number, name: string, description: string): Promise<Layer> {
  return $fetch<Layer>(under(brainId, "/layers"), { method: "POST", body: { name, description } });
}

export function changeLayer(brainId: number, id: number, change: { description?: string; position?: number }): Promise<Layer> {
  return $fetch<Layer>(under(brainId, `/layers/${id}`), { method: "PATCH", body: change });
}

export function changeSubject(brainId: number, id: number, description: string): Promise<Subject> {
  return $fetch<Subject>(under(brainId, `/subjects/${id}`), { method: "PATCH", body: { description } });
}

// Whether a refusal is the brain's conflict: a version written over a row someone else moved first.
export function isConflict(error: unknown): boolean {
  const data = (error as { data?: { code?: string } } | null)?.data;

  return data?.code === "conflict";
}
