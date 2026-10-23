import type { LocationQuery, LocationQueryRaw } from "vue-router";

import type { MemoryFilter } from "./brain-api";

// Where the screen stands, read from its address and written back to it (docs/design.md, 「The screen」): the
// brain, the view, the filters and the memory open. An address handed on opens the same place.

export const VIEWS = ["list", "map", "bubbles", "timeline", "book"] as const;
export type View = (typeof VIEWS)[number];

// The status choices of the toolbar: the live rows by default, each status, or every row.
export const STATUS_CHOICES = ["live", "active", "pending", "done", "invalidated", "superseded", "all"] as const;
export type StatusChoice = (typeof STATUS_CHOICES)[number];

// Every type but episodes by default: episodes gather on every conversation and would cover the rest.
export const EVERY_TYPE_BUT_EPISODES = "-episode";
export const EVERY_TYPE = "*";

export const ORDERS = ["-validFrom", "validFrom", "-importance", "-hits", "position"] as const;
export type Order = (typeof ORDERS)[number];

export interface Place {
  brainId: number | null;
  view: View;
  status: StatusChoice;
  type: string;
  layer: number | "unfiled" | null;
  subject: number | null;
  key: string;
  label: string;
  q: string;
  order: Order;
  memory: number | null;
  by: "type" | "layer";
}

function one(query: LocationQuery, name: string): string {
  const value = query[name];
  const first = Array.isArray(value) ? value[0] : value;

  return typeof first === "string" ? first : "";
}

function id(text: string): number | null {
  return /^\d+$/.test(text) ? Number(text) : null;
}

function among<T extends string>(choices: readonly T[], text: string, fallback: T): T {
  return (choices as readonly string[]).includes(text) ? (text as T) : fallback;
}

// The place an address names: `/brain/<brainId>/<view>` and the filters in the query.
export function placeOf(segments: string[], query: LocationQuery): Place {
  const layer = one(query, "layer");

  return {
    brainId: id(segments[0] ?? ""),
    view: among(VIEWS, segments[1] ?? "", "list"),
    status: among(STATUS_CHOICES, one(query, "status"), "live"),
    type: one(query, "type") || EVERY_TYPE_BUT_EPISODES,
    layer: layer === "unfiled" ? "unfiled" : id(layer),
    subject: id(one(query, "subject")),
    key: one(query, "key"),
    label: one(query, "label"),
    q: one(query, "q"),
    order: among(ORDERS, one(query, "order"), "-validFrom"),
    memory: id(one(query, "memory")),
    by: one(query, "by") === "layer" ? "layer" : "type"
  };
}

// The path a place stands at.
export function pathOf(place: Place): string {
  if (place.brainId === null) return "/brain";

  return place.view === "list" ? `/brain/${place.brainId}` : `/brain/${place.brainId}/${place.view}`;
}

// The query a place carries, its defaults left out so a plain address stays plain.
export function queryOf(place: Place): LocationQueryRaw {
  const query: LocationQueryRaw = {};

  if (place.status !== "live") query.status = place.status;
  if (place.type !== EVERY_TYPE_BUT_EPISODES) query.type = place.type;
  if (place.layer !== null) query.layer = String(place.layer);
  if (place.subject !== null) query.subject = String(place.subject);
  if (place.key !== "") query.key = place.key;
  if (place.label !== "") query.label = place.label;
  if (place.q !== "") query.q = place.q;
  if (place.order !== "-validFrom") query.order = place.order;
  if (place.memory !== null) query.memory = String(place.memory);
  if (place.by !== "type") query.by = place.by;

  return query;
}

// The status a choice asks the brain for, or none for every row.
export function statusAsked(choice: StatusChoice): string | undefined {
  if (choice === "all") return undefined;

  return choice === "live" ? "active,pending" : choice;
}

// The memory list's filters a place names. `without` leaves one filter out, as the counts beside that
// filter's own choices need.
export function filterOf(place: Place, without: "status" | "type" | null = null): MemoryFilter {
  const filter: MemoryFilter = {};

  if (without !== "status") filter.status = statusAsked(place.status);
  if (without !== "type") {
    if (place.type === EVERY_TYPE_BUT_EPISODES) filter.excludeTypes = "episode";
    else if (place.type !== EVERY_TYPE) filter.type = place.type;
  }
  if (place.layer === "unfiled") filter.unfiled = true;
  else if (place.layer !== null) filter.layerId = place.layer;
  if (place.subject !== null) filter.subjectId = place.subject;
  if (place.key !== "") filter.key = place.key;
  if (place.label !== "") filter.label = place.label;
  if (place.q !== "") filter.q = place.q;

  return filter;
}

// The order the list is read in: a layer's memories by their position unless another order was chosen.
export function orderOf(place: Place): string {
  return place.layer !== null && place.order === "-validFrom" ? "position" : place.order;
}
