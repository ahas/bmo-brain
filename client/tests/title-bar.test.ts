import { expect, test } from "bun:test";

// The screen under the console's one title bar (docs/design.md, 「The screen」), read off the files the bundle ships.

const entry = await Bun.file(new URL("../console/entry.ts", import.meta.url)).text();
const page = await Bun.file(new URL("../app/pages/brain/[...path].vue", import.meta.url)).text();
const index = await Bun.file(new URL("../app/components/BrainIndex.vue", import.meta.url)).text();

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

test("the reading toggle stands at the toolbar's end, and the create button after it as its last cell", () => {
  const from = page.indexOf("<ToolBar");
  const to = page.indexOf("</ToolBar>", from);
  const toolbar = page.slice(from, to);
  const group = toolbar.lastIndexOf("<ActionGroup");
  const create = toolbar.lastIndexOf("<CreateButton");

  expect(group).toBeGreaterThan(-1);
  expect(toolbar.slice(group)).toContain('<ActionGroup class="ms-auto" :actions="screenActions" />');
  expect(create).toBeGreaterThan(group);
  expect(page.match(/<CreateButton\b/g)?.length).toBe(1);
  expect(page).toContain('label: t("brain.action.reading")');
});

test("teaching and adding a layer are the create button's menu, and no other button creates", () => {
  const items = page.slice(page.indexOf("const createItems"), page.indexOf("]);", page.indexOf("const createItems")));
  const actions = page.slice(page.indexOf("const screenActions"), page.indexOf("const createItems"));

  expect(items).toContain('label: t("brain.action.teach")');
  expect(items).toContain('label: t("brain.action.layer_add")');
  expect(actions).not.toContain("brain.action.teach");
  expect(index).not.toContain("brain.action.layer_add");
});

test("the status, the type and the order stand behind the filters button, and a phone chooses the brain in the index", () => {
  const toolbar = page.slice(page.indexOf("<ToolBar"), page.indexOf("</ToolBar>"));
  const filters = toolbar.slice(toolbar.indexOf("<template #content>"), toolbar.indexOf("</template>", toolbar.indexOf("<template #content>")));
  const drawer = page.slice(page.indexOf("<UDrawer"), page.indexOf("</UDrawer>"));

  for (const choice of ["status", "type", "order"]) expect(filters).toContain(`t('brain.label.${choice}')`);
  expect(drawer).toContain('<ChoiceSelect v-bind="brainSelect"');
});
