import { expect, test } from "bun:test";

import { holdsHangul, wordChange } from "../app/domain/brain-words";

test("a change keeps the common words and marks what was added and removed", () => {
  const pieces = wordChange("likes green tea", "likes black tea");

  expect(pieces).toEqual([
    { text: "likes ", change: "same" },
    { text: "green", change: "removed" },
    { text: "black", change: "added" },
    { text: " tea", change: "same" }
  ]);
});

test("words with Hangul are told apart from English ones", () => {
  expect(holdsHangul("Prefers tea")).toBe(false);
  expect(holdsHangul("차를 좋아함")).toBe(true);
});
