import { expect, test } from "bun:test";

// The screen under the console's one title bar (docs/design.md, 「The screen」), read off the files the bundle ships.

const entry = await Bun.file(new URL("../console/entry.ts", import.meta.url)).text();
const page = await Bun.file(new URL("../app/pages/brain/[...path].vue", import.meta.url)).text();

test("the screen's route asks for the console's title bar", () => {
  expect(entry).toContain("consoleTitleBar: true");
});

test("the screen draws a bar of its own only where the console offers none", () => {
  expect(page).toContain('inject<{ screenActions: string } | null>("bmo-console-title-bar", null)');
  expect(page).toContain('<template v-if="bar === null" #header>');
  expect(page.match(/<UDashboardNavbar\b/g)?.length).toBe(1);
});

test("the reading toggle and teaching stand in the bar's place for the screen's actions, deferred", () => {
  const from = page.indexOf('<Teleport v-if="bar === null || placed" defer :to="bar?.screenActions');
  const to = page.indexOf("</Teleport>", from);
  const moved = page.slice(from, to);

  expect(from).toBeGreaterThan(-1);
  expect(page).toContain(':disabled="bar === null"');
  expect(moved).toContain("brain.action.reading");
  expect(moved).toContain("brain.action.teach");
});
