import { expect, test } from "bun:test";

import { holdsOtherScript, wordChange } from "../app/domain/brain-words";

test("a change keeps the common words and marks what was added and removed", () => {
  const pieces = wordChange("likes green tea", "likes black tea");

  expect(pieces).toEqual([
    { text: "likes ", change: "same" },
    { text: "green", change: "removed" },
    { text: "black", change: "added" },
    { text: " tea", change: "same" }
  ]);
});

test("words in Hangul, kana or Han are told apart from English ones", () => {
  expect(holdsOtherScript("Prefers tea")).toBe(false);
  expect(holdsOtherScript("Prefers café au lait — 2 cups")).toBe(false);
  expect(holdsOtherScript("차를 좋아함")).toBe(true);
  expect(holdsOtherScript("お茶が好き")).toBe(true);
  expect(holdsOtherScript("カフェ")).toBe(true);
  expect(holdsOtherScript("喜欢喝茶")).toBe(true);
});
