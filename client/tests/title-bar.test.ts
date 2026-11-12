import { expect, test } from "bun:test";

// The screen under the console's one title bar (docs/design.md, 「The screen」), read off the files the bundle ships.

const entry = await Bun.file(new URL("../console/entry.ts", import.meta.url)).text();
const page = await Bun.file(new URL("../app/pages/brain/[...path].vue", import.meta.url)).text();

test("the screen's route asks for the console's title bar", () => {
  expect(entry).toContain("consoleTitleBar: true");
});

test("the screen draws a bar of its own only where the console offers none", () => {
  expect(page).toContain('inject<object | null>("bmo-console-title-bar", null)');
  expect(page).toContain('<template v-if="bar === null" #header>');
  expect(page.match(/<UDashboardNavbar\b/g)?.length).toBe(1);
});

test("nothing is laid into the console's bar", () => {
  expect(page).not.toContain("<Teleport");
  expect(page).not.toContain("screenActions:");
  expect(page).not.toContain("screenLead");
});

test("the reading toggle and teaching stand at the toolbar's end", () => {
  const from = page.indexOf("<ToolBar");
  const to = page.indexOf("</ToolBar>", from);
  const toolbar = page.slice(from, to);
  const group = toolbar.lastIndexOf("<ActionGroup");

  expect(group).toBeGreaterThan(-1);
  expect(toolbar.slice(group)).toContain('<ActionGroup class="ms-auto" :actions="screenActions" />');
  expect(page).toContain('label: t("brain.action.reading")');
  expect(page).toContain('label: t("brain.action.teach")');
});

test("the status, the type and the order stand behind the filters button, and a phone chooses the brain in the index", () => {
  const toolbar = page.slice(page.indexOf("<ToolBar"), page.indexOf("</ToolBar>"));
  const filters = toolbar.slice(toolbar.indexOf("<template #content>"), toolbar.indexOf("</template>", toolbar.indexOf("<template #content>")));
  const drawer = page.slice(page.indexOf("<UDrawer"), page.indexOf("</UDrawer>"));

  for (const choice of ["status", "type", "order"]) expect(filters).toContain(`t('brain.label.${choice}')`);
  expect(drawer).toContain('<USelect v-bind="brainSelect"');
});
