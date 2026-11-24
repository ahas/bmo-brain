// The agent's sleep as the screen reads it (docs/design.md, 「Sleep」): its summary is a live `reflection` labelled
// `sleep`, the label's value the date it slept for; the newest one is the last night's.

import type { Memory } from "./brain-api";
import type { MemoryFilter } from "./brain-api";

export const SLEEP_LABEL = "sleep";

/** The memory list's filter that finds the sleep summaries, newest first under the list's own order. */
export function sleepFilter(): MemoryFilter {
  return { type: "reflection", status: "active", label: SLEEP_LABEL };
}

/** The date a summary slept for: its label's value, or the day it was written where the label holds none. */
export function sleptOn(memory: Pick<Memory, "labels" | "createdAt">, label: string): string {
  const said = memory.labels?.[label]?.trim() ?? "";
  return said === "" ? new Date(memory.createdAt).toISOString().slice(0, 10) : said;
}
