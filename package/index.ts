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
export type Restoration = Schemas["Restoration"];
export type Rules = Schemas["Rules"];
export type Subject = Schemas["Subject"];
export type SubjectChange = Schemas["SubjectChange"];
export type SubjectKey = Schemas["SubjectKey"];
export type SubjectPage = Schemas["SubjectPage"];
export type SubjectRef = Schemas["SubjectRef"];
export type Tally = Schemas["Tally"];
export type TallyBy = Schemas["TallyBy"];
export type TypeCount = Schemas["TypeCount"];
export type Consolidated = Schemas["Consolidated"];
export type Learned = Schemas["Learned"];
export type Learning = Schemas["Learning"];
export type SimilarAsk = Schemas["SimilarAsk"];
export type Event = Schemas["Event"];
export type EventPage = Schemas["EventPage"];
export type Health = Schemas["Health"];
export type WireError = Schemas["WireError"];
export type ConsoleManifest = Schemas["ConsoleManifest"];
export type Role = Schemas["Role"];

export type BrainClient = Client<paths>;

/**
 * A client of the brain at `baseUrl`, sending `token` as its bearer token and asking for JSON, so
 * `GET /events` answers a page. Each call is a route of the OpenAPI document, typed by it:
 * `client.GET("/brains/{brainId}", { params: { path: { brainId } } })`. Reading the live event
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
