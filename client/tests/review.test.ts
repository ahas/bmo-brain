import { expect, test } from "bun:test";

import { candidatesOf, isBase, sourceOf } from "../app/domain/brain-review";

import type { Memory } from "../app/domain/brain-api";

// The review of the base's candidates (docs/design.md, 「Raising a memory to the base」): the base told among the
// brains by its keys, a candidate's source read off its labels, and the candidates the pending raised rows alone.

function row(id: number, status: Memory["status"], labels: Record<string, string>): Memory {
  return { id, status, labels } as Memory;
}

test("the base is the org brain filed under the platform's keys alone", () => {
  expect(isBase({ kind: "org", accountKey: "platform", agentKey: "platform" })).toBe(true);
  expect(isBase({ kind: "agent", accountKey: "platform", agentKey: "platform" })).toBe(false);
  expect(isBase({ kind: "org", accountKey: "company-a", agentKey: "platform" })).toBe(false);
  expect(isBase(undefined)).toBe(false);
});

test("a candidate's source is its labels' brain and memory, none where either does not read", () => {
  expect(sourceOf(row(1, "pending", { sourceBrainId: "3", sourceMemoryId: "40" }))).toEqual({ brainId: 3, memoryId: 40 });
  expect(sourceOf(row(1, "pending", { sourceBrainId: "3" }))).toBeNull();
  expect(sourceOf(row(1, "pending", { sourceBrainId: "x", sourceMemoryId: "40" }))).toBeNull();
});

test("the candidates are the pending rows a source raised, oldest first", () => {
  const raised = { sourceBrainId: "3", sourceMemoryId: "40" };
  const rows = [row(9, "pending", raised), row(2, "pending", raised), row(5, "active", raised), row(4, "pending", {})];

  expect(candidatesOf(rows).map((one) => one.id)).toEqual([2, 9]);
});
