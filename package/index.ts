// The brain service's contract in TypeScript (docs/boundary-api.md, "The client"): the types
// openapi-typescript generates from docs/openapi.json, and openapi-fetch's typed calls over them, each
// route by its path.

import createClient, { type Client } from "openapi-fetch";

import type { components, paths } from "./openapi";

export type { components, operations, paths } from "./openapi";

type Schemas = components["schemas"];

export type Brain = Schemas["Brain"];
export type BrainPage = Schemas["BrainPage"];
export type Confirmations = Schemas["Confirmations"];
export type Fold = Schemas["Fold"];
export type Folded = Schemas["Folded"];
export type Hits = Schemas["Hits"];
export type Injected = Schemas["Injected"];
export type Layer = Schemas["Layer"];
export type LayerChange = Schemas["LayerChange"];
export type LayerPage = Schemas["LayerPage"];
export type Memory = Schemas["Memory"];
export type MemoryMove = Schemas["MemoryMove"];
export type MemoryPage = Schemas["MemoryPage"];
export type MemoryVersion = Schemas["MemoryVersion"];
export type NewBrain = Schemas["NewBrain"];
export type NewLayer = Schemas["NewLayer"];
export type NewMemory = Schemas["NewMemory"];
export type NewSubject = Schemas["NewSubject"];
export type Recall = Schemas["Recall"];
export type RecallAsk = Schemas["RecallAsk"];
export type RecallChoice = Schemas["RecallChoice"];
export type Recorded = Schemas["Recorded"];
export type Rules = Schemas["Rules"];
export type Subject = Schemas["Subject"];
export type SubjectChange = Schemas["SubjectChange"];
export type SubjectKey = Schemas["SubjectKey"];
export type SubjectPage = Schemas["SubjectPage"];
export type SubjectRef = Schemas["SubjectRef"];
export type TypeCount = Schemas["TypeCount"];
export type Consolidated = Schemas["Consolidated"];
export type Learned = Schemas["Learned"];
export type Learning = Schemas["Learning"];
export type SimilarAsk = Schemas["SimilarAsk"];
export type Drift = Schemas["Drift"];
export type DriftChange = Schemas["DriftChange"];
export type Event = Schemas["Event"];
export type EventPage = Schemas["EventPage"];
export type Fragment = Schemas["Fragment"];
export type FragmentHits = Schemas["FragmentHits"];
export type FragmentHitsRecorded = Schemas["FragmentHitsRecorded"];
export type FragmentKind = Schemas["FragmentKind"];
export type FragmentKindPage = Schemas["FragmentKindPage"];
export type FragmentMerge = Schemas["FragmentMerge"];
export type FragmentPage = Schemas["FragmentPage"];
export type FragmentPiece = Schemas["FragmentPiece"];
export type FragmentSplit = Schemas["FragmentSplit"];
export type FragmentState = Schemas["FragmentState"];
export type FragmentSwitch = Schemas["FragmentSwitch"];
export type FragmentWrite = Schemas["FragmentWrite"];
export type Health = Schemas["Health"];
export type KindWords = Schemas["KindWords"];
export type Knowledge = Schemas["Knowledge"];
export type Library = Schemas["Library"];
export type LibraryDocument = Schemas["LibraryDocument"];
export type LibraryDocumentPut = Schemas["LibraryDocumentPut"];
export type LibraryPage = Schemas["LibraryPage"];
export type LibraryPut = Schemas["LibraryPut"];
export type LibrarySync = Schemas["LibrarySync"];
export type LibrarySyncAsk = Schemas["LibrarySyncAsk"];
export type Lock = Schemas["Lock"];
export type LockPage = Schemas["LockPage"];
export type ScopedMemory = Schemas["ScopedMemory"];
export type ScopedMemoryChange = Schemas["ScopedMemoryChange"];
export type ScopedMemoryMatch = Schemas["ScopedMemoryMatch"];
export type ScopedMemoryState = Schemas["ScopedMemoryState"];
export type NewLock = Schemas["NewLock"];
export type NewScopedMemory = Schemas["NewScopedMemory"];
export type NewScopedMemoryMatch = Schemas["NewScopedMemoryMatch"];
export type NewNote = Schemas["NewNote"];
export type NewReminder = Schemas["NewReminder"];
export type NewWatch = Schemas["NewWatch"];
export type Note = Schemas["Note"];
export type NotePage = Schemas["NotePage"];
export type Reminder = Schemas["Reminder"];
export type ReminderPage = Schemas["ReminderPage"];
export type ScopeChange = Schemas["ScopeChange"];
export type ScopeCounts = Schemas["ScopeCounts"];
export type ScopeHolding = Schemas["ScopeHolding"];
export type ScopeState = Schemas["ScopeState"];
export type Watch = Schemas["Watch"];
export type WatchChange = Schemas["WatchChange"];
export type WatchPage = Schemas["WatchPage"];
export type WatchState = Schemas["WatchState"];
export type WireError = Schemas["WireError"];

export type BrainClient = Client<paths>;

/**
 * A client of the brain at `baseUrl`, sending `token` as its bearer token and asking for JSON, so
 * `GET /events` answers a page. Each call is a route of the OpenAPI document, typed by it:
 * `client.GET("/memories/{memoryId}", { params: { path: { memoryId } } })`. Reading the live event
 * stream is the same route without the JSON answer, for a caller that reads server-sent events itself.
 */
export function brainClient(
  baseUrl: string,
  token: string,
  fetcher?: typeof globalThis.fetch,
): BrainClient {
  return createClient<paths>({
    baseUrl,
    headers: { Authorization: `Bearer ${token}`, Accept: "application/json" },
    ...(fetcher === undefined ? {} : { fetch: fetcher }),
  });
}
