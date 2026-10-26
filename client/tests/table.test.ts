import { expect, test } from "bun:test";

import { MEMORY_COLUMNS, STACK_BELOW_PX, leastTableWidth } from "../app/domain/brain-table";

// The memory table's column rules (docs/design.md, 「The screen」).

// The Tailwind width a class names, in rem: `w-20` is 5rem, `min-w-32` 8rem.
function remOf(classes: string, prefix: string): number | null {
  const found = classes.split(" ").find((one) => one.startsWith(prefix) && /^\d+(\.\d+)?$/.test(one.slice(prefix.length)));

  return found === undefined ? null : Number(found.slice(prefix.length)) / 4;
}

test("one column takes the room left, the text, from a floor of 8rem", () => {
  const flexible = MEMORY_COLUMNS.filter((column) => column.flexible);

  expect(flexible.map((column) => column.id)).toEqual(["text"]);
  expect(flexible[0]!.rem).toBe(8);
  expect(remOf(flexible[0]!.th, "min-w-")).toBe(8);
  expect(flexible[0]!.th.split(" ")).toContain("w-full");
});

test("every other column keeps the one width its rem names, its cells no wider", () => {
  for (const column of MEMORY_COLUMNS.filter((one) => !one.flexible)) {
    expect(remOf(column.th, "w-")).toBe(column.rem);
    expect(remOf(column.td, "max-w-")).toBe(column.rem);
  }
});

test("the table stacks before its columns would be squeezed under their own widths", () => {
  expect(MEMORY_COLUMNS.map((column) => column.id)).toEqual(["type", "text", "layer", "status", "importance", "validFrom"]);
  expect(leastTableWidth()).toBe(512);
  expect(leastTableWidth()).toBeLessThan(STACK_BELOW_PX);
});
