import { afterAll, beforeAll, beforeEach, describe, expect, test } from "bun:test";
import { spawn } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";

import { chromium } from "playwright-core";

import type { ChildProcess } from "node:child_process";
import type { Browser, Locator, Page, Route } from "playwright-core";

// The brain's screen (docs/design.md, 「The screen」) driven in a real Chromium. The pointer's three carries, with
// the mouse: a piece of the layer view carried to another place, a layer carried among the layers, and a piece
// dropped on another layer, on the unfiled or on its own layer. And its selects, at a wide window and a phone's: a
// list of more than seven choices opens with a search field, on an edge of its trigger and inside the window, one of
// seven or fewer without. And an org brain, at a wide window and a phone's: read and never written, one line saying
// where it is written, the create button disabled and saying the same, and no carry, move, description, edit or status
// move standing, while an agent's brain keeps its create button. And the last night's sleep, at a wide window and a
// phone's: its card folded at the head of the memories, opened by a press, and its row opened from it, read in the
// newest-first order the brain's grammar has, the stand-in refusing any other. And the memories read a page at a
// time, at a wide window and a phone's: the next page read by the scroll alone, nothing to press and nothing under
// the last page, and a page that fails keeping what was read and read again from its retry. The screen
// stands alone on the host under `host/`,
// and the brain's routes are answered by a stand-in holding an agent's brain, which keeps every write, so what the
// screen wrote is read back and the screen read again shows it, and the base beside it. Skipped where the browser
// playwright-core pins is not on the machine.

function browserReady(): boolean {
  try {
    return existsSync(chromium.executablePath());
  } catch {
    return false;
  }
}

const ready = browserReady();
const CLIENT = new URL("../..", import.meta.url).pathname;
const HOST = new URL("./host", import.meta.url).pathname;
const RELAY = "/relay/brain";
const WORDS = JSON.parse(readFileSync(new URL("../../i18n/locales/ko.json", import.meta.url), "utf8")) as {
  brain: { title: Record<string, string>; label: Record<string, string>; action: Record<string, string>; hint: Record<string, string> };
};
// The layer's words the host answers beside the screen's own (`host/i18n/`).
const COMMON = JSON.parse(readFileSync(new URL("./host/i18n/locales/ko.json", import.meta.url), "utf8")) as {
  common: { placeholder: { search: string }; action: { retry: string } };
};

// --- The stand-in brain ---

interface Row {
  id: number;
  type: string;
  text: string;
  layerId: number | null;
  position: number;
  status: "active" | "superseded";
  labels?: Record<string, string>;
}

interface LayerRow {
  id: number;
  name: string;
  position: number;
}

interface Write {
  method: string;
  path: string;
  body: Record<string, unknown>;
}

interface Read {
  path: string;
  query: URLSearchParams;
}

const AT = 1_790_000_000_000;
let layers: LayerRow[] = [];
let rows: Row[] = [];
let writes: Write[] = [];
let reads: Read[] = [];
// Whether the next page asked is answered with a failure, once.
let failNextPage = false;

// The base, brain 2, an org brain the screen reads and never writes (docs/design.md, 「The screen」): one layer and one
// memory in it.
const ORG_TEXT = "Read the whole log before retrying a failed step.";

function orgMemory(): Record<string, unknown> {
  return { ...memoryOf({ id: 201, type: "tip", text: ORG_TEXT, layerId: 21, position: 1, status: "active" }), brainId: 2 };
}

function seed(): void {
  layers = [
    { id: 1, name: "persona-core", position: 1 },
    { id: 2, name: "lore", position: 2 },
    { id: 3, name: "notes", position: 3 }
  ];
  rows = [
    { id: 1, type: "persona", text: "persona one", layerId: 1, position: 1, status: "active" },
    { id: 2, type: "persona", text: "persona two", layerId: 1, position: 2, status: "active" },
    { id: 3, type: "fact", text: "lore one", layerId: 2, position: 1, status: "active" },
    { id: 4, type: "fact", text: "lore two", layerId: 2, position: 2, status: "active" },
    { id: 5, type: "fact", text: "lore three", layerId: 2, position: 3, status: "active" },
    { id: 6, type: "fact", text: "lore four", layerId: 2, position: 4, status: "active" },
    { id: 7, type: "fact", text: "note one", layerId: 3, position: 1, status: "active" },
    { id: 8, type: "fact", text: "unfiled one", layerId: null, position: 0, status: "active" }
  ];
  writes = [];
  reads = [];
  failNextPage = false;
}

function memoryOf(row: Row): Record<string, unknown> {
  return {
    ...row,
    brainId: 1,
    actorName: "writer-a",
    consolidatedAt: null,
    createdAt: AT,
    importance: 5,
    key: null,
    labels: row.labels ?? {},
    lastConfirmedAt: null,
    lastUsedAt: null,
    lineageId: row.id,
    reason: null,
    recallCondition: null,
    releaseCondition: null,
    subjects: [],
    supersededAt: null,
    supersededBy: null,
    supersedes: [],
    useCount: 0,
    validFrom: AT,
    validTo: null
  };
}

function layerOf(layer: LayerRow): Record<string, unknown> {
  return { ...layer, brainId: 1, createdAt: AT, description: "" };
}

const byPosition = <T extends { id: number; position: number }>(a: T, b: T) => a.position - b.position || a.id - b.id;

// The order grammar of the brain's memory list (docs/boundary-api.md, 「Memories」): a field, ascending, or
// descending with a leading `-`. A stand-in that took any word for it would pass a screen asking in a key the
// brain refuses.
const ORDER_FIELDS = ["id", "position", "validFrom", "importance", "useCount"];

function orderAccepted(order: string): boolean {
  return ORDER_FIELDS.includes(order.replace(/^-/, ""));
}

function listed(query: URLSearchParams): Row[] {
  const statuses = query.get("status")?.split(",");
  const types = query.get("type")?.split(",");
  const layerId = query.get("layerId");
  const label = query.get("label");

  return rows
    .filter((row) => label === null || (row.labels ?? {})[label] !== undefined)
    .filter((row) => (statuses === undefined ? row.status === "active" : statuses.includes(row.status)))
    .filter((row) => types === undefined || types.includes(row.type))
    .filter((row) => (layerId !== null ? row.layerId === Number(layerId) : query.get("unfiled") !== "true" || row.layerId === null))
    .sort(byPosition);
}

// A version as the brain writes it: the old row superseded, the new one at the layer and position given, the old
// ones where none is, and after the last of the layer it moves into where it moves and names no position.
function versioned(id: number, body: Record<string, unknown>): Row {
  const old = rows.find((row) => row.id === id && row.status === "active")!;
  const layerId = "layerId" in body ? (body.layerId as number | null) : old.layerId;
  const last = Math.max(-1, ...rows.filter((row) => row.layerId === layerId).map((row) => row.position));
  const position = typeof body.position === "number" ? body.position : layerId === old.layerId ? old.position : last + 1;
  const row: Row = { ...old, id: Math.max(...rows.map((one) => one.id)) + 1, layerId, position };

  old.status = "superseded";
  rows.push(row);

  return row;
}

function json(route: Route, body: unknown): Promise<void> {
  return route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(body) });
}

async function answer(route: Route): Promise<void> {
  const request = route.request();
  const url = new URL(request.url());
  const path = url.pathname.slice(RELAY.length);
  const method = request.method();

  if (method === "GET") reads.push({ path, query: url.searchParams });

  if (method !== "GET") {
    const body = (request.postDataJSON() ?? {}) as Record<string, unknown>;

    writes.push({ method, path, body });

    const version = /^\/brains\/1\/memories\/(\d+)\/versions$/.exec(path);
    const layer = /^\/brains\/1\/layers\/(\d+)$/.exec(path);

    if (method === "POST" && version !== null) return json(route, memoryOf(versioned(Number(version[1]), body)));

    if (method === "PATCH" && layer !== null) {
      const changed = layers.find((one) => one.id === Number(layer[1]))!;

      changed.position = typeof body.position === "number" ? body.position : changed.position;

      return json(route, layerOf(changed));
    }

    return route.fulfill({ status: 404 });
  }

  const order = url.searchParams.get("order");

  if (order !== null && path.endsWith("/memories") && !orderAccepted(order))
    return route.fulfill({
      status: 400,
      contentType: "application/json",
      body: JSON.stringify({
        code: "bad-request",
        error: `order is "${order}"; it is id, position, validFrom, importance or useCount, descending with a leading -.`
      })
    });

  switch (path) {
    case "/events":
      // A stream that names an hour to wait before it is opened again, so the page holds no stream and reads afresh
      // only when the test loads it.
      return route.fulfill({ status: 200, contentType: "text/event-stream", body: "retry: 3600000\n\n" });
    case "/brains":
      return json(route, {
        items: [
          { id: 1, kind: "agent", accountKey: "account-a", agentKey: "writer-a", name: "brain-a", createdAt: AT },
          { id: 2, kind: "org", accountKey: "platform", agentKey: "platform", name: "base", createdAt: AT }
        ],
        more: false
      });
    case "/brains/2/layers":
      return json(route, { items: [{ ...layerOf({ id: 21, name: "rules", position: 1 }), brainId: 2 }], more: false });
    case "/brains/2/subjects":
      return json(route, { items: [], more: false });
    case "/brains/2/types":
      return json(route, [{ type: "tip", reserved: false, count: 1 }]);
    case "/brains/2/tallies":
      return json(route, []);
    case "/brains/2/memories":
      return json(route, { items: [orgMemory()], more: false });
    case "/brains/2/memories/201":
      return json(route, orgMemory());
    case "/brains/2/lineages/201":
      return json(route, [orgMemory()]);
    case "/brains/1/layers":
      return json(route, { items: [...layers].sort(byPosition).map(layerOf), more: false });
    case "/brains/1/subjects":
      return json(route, { items: [], more: false });
    case "/brains/1/types":
      return json(route, [...new Set(rows.map((row) => row.type))].map((type) => ({ type, reserved: type === "persona", count: 1 })));
    case "/brains/1/tallies":
      return json(route, []);
    case "/brains/1/memories": {
      // A page starts past the memory the cursor names, in the listed order, and says whether more stand past it.
      const items = listed(url.searchParams);
      const after = url.searchParams.get("after");
      const from = after === null ? 0 : items.findIndex((row) => row.id === Number(after)) + 1;
      const limit = Number(url.searchParams.get("limit") ?? "50");

      if (after !== null && failNextPage) {
        failNextPage = false;

        // A status the fetcher does not retry on its own, so the one failure reaches the screen.
        return route.fulfill({ status: 400, contentType: "application/json", body: JSON.stringify({ message: "unavailable" }) });
      }

      return json(route, { items: items.slice(from, from + limit).map(memoryOf), more: items.length > from + limit });
    }
    case "/brains/1/persona":
      return route.fulfill({ status: 200, contentType: "text/plain", body: listed(new URLSearchParams("type=persona&layerId=1")).map((row) => row.text).join("\n\n") });
    default:
      return route.fulfill({ status: 404 });
  }
}

// --- The host and the browser ---

const WIDE = { width: 1400, height: 900 };
// A phone's window, where the screen stands as it does on a phone.
const NARROW = { width: 360, height: 780 };

let host: ChildProcess | null = null;
let origin = "";
let browser: Browser;
let page: Page;

function freePort(): number {
  const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: () => new Response() });
  const port = server.port!;

  void server.stop(true);

  return port;
}

async function until(holds: () => boolean | Promise<boolean>, what: string, ms = 15_000): Promise<void> {
  const end = Date.now() + ms;

  while (!(await holds())) {
    if (Date.now() > end) throw new Error(`${what} did not come within ${ms} ms`);

    await Bun.sleep(25);
  }
}

async function answers(address: string): Promise<boolean> {
  try {
    return (await fetch(address)).ok;
  } catch {
    return false;
  }
}

async function open(place: string): Promise<void> {
  await page.goto(`${origin}/brain/1/book${place}`, { waitUntil: "load" });
  await page.waitForSelector("[data-piece]");
  await page.waitForSelector("[data-layer]");
}

const piece = (text: string) => page.locator("[data-piece]", { hasText: text });
const layerRow = (name: string) => page.locator("[data-layer]", { hasText: name });
const shown = () => page.locator("[data-piece] button").allInnerTexts().then((texts) => texts.filter((text) => text !== ""));
const layerNames = () => page.locator("[data-layer] .truncate.font-medium").allInnerTexts();

// The line each row shows: over it where a carried thing would land before it, under it where after it.
async function lines(selector: string): Promise<string[]> {
  return page.locator(selector).evaluateAll((found) =>
    found.map((row) => {
      const shadow = getComputedStyle(row).boxShadow;

      return shadow.includes("0px 2px 0px 0px inset") ? "before" : shadow.includes("0px -2px 0px 0px inset") ? "after" : "";
    })
  );
}

// The lines once the pointer's last move has been taken: the browser hands the page a drag's moves as it gets to
// them, so a line is read again until it shows what is expected, or the time is up and the last reading stands.
async function settledLines(selector: string, expected: string[]): Promise<string[]> {
  let read: string[] = [];

  await until(async () => {
    read = await lines(selector);

    return JSON.stringify(read) === JSON.stringify(expected);
  }, "the line", 2_000).catch(() => undefined);

  return read;
}

// Whether a row stands highlighted as the place a carried piece would go: the ring drawn about it.
async function ringed(row: Locator): Promise<boolean> {
  return row.evaluate((element) => getComputedStyle(element).boxShadow.includes("0px 0px 0px 1px inset"));
}

// A select of the screen, by the name its trigger, a button, carries.
const chosen = (name: string) => page.getByRole("button", { name, exact: true });

// The filters' popover opened, unless it already stands open.
async function openFilters(): Promise<void> {
  if (await chosen(WORDS.brain.label.status!).isVisible()) return;

  await page.getByRole("button", { name: WORDS.brain.action.filters!, exact: true }).click();
  await chosen(WORDS.brain.label.status!).waitFor();
}

type Box = { x: number; y: number; width: number; height: number };

/** The open list's box once its opening animation has ended: a list measured while it opens stands narrower. */
async function openList(): Promise<Box> {
  return page.getByRole("listbox").evaluate(async (element) => {
    const content = element.closest("[data-slot='content']") ?? element;

    await Promise.all(content.getAnimations({ subtree: true }).map((animation) => animation.finished));

    const { x, y, width, height } = content.getBoundingClientRect();

    return { x, y, width, height };
  });
}

/** The mouse pressed on a row and moved off the press, so the browser starts carrying it. */
async function pick(row: Locator): Promise<void> {
  const box = (await row.boundingBox())!;

  await page.mouse.move(box.x + 40, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + 40, box.y + box.height / 2 - 6, { steps: 3 });
}

/**
 * The carried thing brought over a row: its upper edge, where it lands before the row, or its middle. A browser
 * hands the page a drag over on some moves and not others, and on its own clock while the pointer rests, which a
 * driven pointer does not keep; so the pointer ends on a few moves where it rests, and the page takes one there.
 */
async function bring(row: Locator, at: "upper" | "middle"): Promise<void> {
  const box = (await row.boundingBox())!;
  const y = at === "upper" ? box.y + 4 : box.y + box.height / 2;

  await page.mouse.move(box.x + 40, y, { steps: 8 });

  for (const x of [41, 40, 41, 40]) await page.mouse.move(box.x + x, y);
}

describe.skipIf(!ready)("the brain's screen in a browser", () => {
  beforeAll(async () => {
    const port = freePort();

    origin = `http://127.0.0.1:${port}`;
    // Run by Bun itself, in a process group of its own, so the host and everything it starts go down together; what
    // it says is kept for the message of a start that fails. A shell stands at the group's head and ends the whole
    // group once its input closes, which it does the moment this run ends however it ends, so a run killed from
    // outside leaves no host behind. Nuxt's dev server locks its build folder, and a host an older run left alive
    // still holds that lock while it serves nothing; the machine's turn runs one browser test at a time, so the host
    // starts past the lock rather than refusing the run.
    const nuxi = [process.execPath, "--bun", `${CLIENT}node_modules/.bin/nuxi`, "dev", HOST];

    host = spawn("sh", ["-c", '"$@" & read -r _; kill 0', "sh", ...nuxi, "--host", "127.0.0.1", "--port", String(port)], {
      cwd: CLIENT,
      detached: true,
      stdio: ["pipe", "pipe", "pipe"],
      env: { ...process.env, NUXT_TELEMETRY_DISABLED: "1", NUXT_IGNORE_LOCK: "1" }
    });

    let said = "";

    host.stdout?.on("data", (chunk) => (said = (said + chunk).slice(-4000)));
    host.stderr?.on("data", (chunk) => (said = (said + chunk).slice(-4000)));
    await until(() => answers(`${origin}/brain/1`), "the screen's host", 120_000).catch((error: Error) => {
      throw new Error(`${error.message}; it said:\n${said}`);
    });

    browser = await chromium.launch();

    const context = await browser.newContext({ viewport: WIDE });

    await context.addCookies([{ name: "nuxt-color-mode", value: "light", url: origin }]);
    page = await context.newPage();
    await page.route((url) => url.pathname.startsWith(`${RELAY}/`), answer);

    // The first load compiles the screen and may reload once its dependencies are bundled.
    seed();
    await page.goto(`${origin}/brain/1/book?layer=2`, { waitUntil: "load", timeout: 120_000 });
    await page.waitForSelector("[data-piece]", { timeout: 120_000 });
  }, 300_000);

  afterAll(async () => {
    await browser?.close();

    if (host?.pid !== undefined) {
      try {
        process.kill(-host.pid, "SIGTERM");
      } catch {
        // Already gone.
      }
    }
  });

  describe("the pointer carries pieces and layers", () => {
    beforeEach(seed);

    test("a piece carried in the layer view lands where its line stood, and stands there read again", async () => {
      await open("?layer=2");
      expect(await shown()).toEqual(["lore one", "lore two", "lore three", "lore four"]);

      await pick(piece("lore four"));
      await bring(piece("lore two"), "upper");
      expect(await settledLines("[data-piece]", ["", "before", "", ""])).toEqual(["", "before", "", ""]);

      // Over the index, the layer view marks no place of its own.
      await bring(layerRow("notes"), "middle");
      expect(await settledLines("[data-piece]", ["", "", "", ""])).toEqual(["", "", "", ""]);

      await bring(piece("lore two"), "upper");
      expect(await settledLines("[data-piece]", ["", "before", "", ""])).toEqual(["", "before", "", ""]);
      await page.mouse.up();
      await until(() => writes.length === 3, "the three writes");
      expect(writes.every((write) => write.method === "POST" && write.path.endsWith("/versions"))).toBe(true);

      await open("?layer=2");
      expect(await shown()).toEqual(["lore one", "lore four", "lore two", "lore three"]);
    }, 60_000);

    test("a layer carried among the layers lands where its line stood, and stands there read again", async () => {
      await open("?layer=2");
      expect(await layerNames()).toEqual(["persona-core", "lore", "notes"]);

      await pick(layerRow("notes"));
      await bring(layerRow("persona-core"), "upper");
      expect(await settledLines("[data-layer]", ["before", "", ""])).toEqual(["before", "", ""]);

      await page.mouse.up();
      await until(() => writes.length === 1, "the layer's write");
      expect(writes[0]!.method).toBe("PATCH");

      await open("?layer=2");
      expect(await layerNames()).toEqual(["notes", "persona-core", "lore"]);
    }, 60_000);

    test("a piece dropped on another layer moves there after its last piece, and on its own layer writes nothing", async () => {
      await open("?layer=2");

      await pick(piece("lore two"));
      await bring(layerRow("lore"), "middle");
      await page.mouse.up();
      // The screen read again comes after anything the drop would have written.
      await open("?layer=2");
      expect(writes).toEqual([]);

      await pick(piece("lore one"));
      await bring(layerRow("notes"), "middle");
      expect(await ringed(layerRow("notes"))).toBe(true);
      expect(await settledLines("[data-piece]", ["", "", "", ""])).toEqual(["", "", "", ""]);

      await page.mouse.up();
      await until(() => writes.length === 1, "the piece's write");
      expect(writes[0]!.body).toMatchObject({ layerId: 3 });
      expect(await ringed(layerRow("notes"))).toBe(false);

      await open("?layer=2");
      expect(await shown()).toEqual(["lore two", "lore three", "lore four"]);
      await open("?layer=3");
      expect(await shown()).toEqual(["note one", "lore one"]);
    }, 60_000);

    test("a persona piece dropped on the unfiled is asked first, and moves only once the question is answered yes", async () => {
      await open("?layer=1");

      const asked = page.getByRole("alertdialog").or(page.getByRole("dialog"));

      await pick(piece("persona one"));
      await bring(page.locator("[data-unfiled]"), "middle");
      expect(await ringed(page.locator("[data-unfiled] button"))).toBe(true);
      await page.mouse.up();
      await asked.waitFor();
      expect(await asked.innerText()).toContain(WORDS.brain.title.persona_save!);
      await page.keyboard.press("Escape");
      await asked.waitFor({ state: "hidden" });
      await open("?layer=1");
      expect(writes).toEqual([]);

      await pick(piece("persona one"));
      await bring(page.locator("[data-unfiled]"), "middle");
      await page.mouse.up();
      await asked.waitFor();
      await asked.getByRole("button").last().click();
      await until(() => writes.length === 1, "the persona piece's write");
      expect(writes[0]!.body).toMatchObject({ layerId: null });

      await open("?layer=unfiled");
      expect(await shown()).toEqual(["unfiled one", "persona one"]);
    }, 60_000);
  });

  describe("a value is chosen in the one select, which searches past seven choices", () => {
    for (const viewport of [{ width: 1280, height: 800 }, NARROW]) {
      test(`at ${viewport.width} wide, the status's eight choices open with a search field and the order's five without`, async () => {
        await page.setViewportSize(viewport);
        await page.goto(`${origin}/brain/1/list`, { waitUntil: "load" });
        // A row the stand-in answered stands once the page has come alive and read the brain.
        await page.getByText("unfiled one").first().waitFor();

        const search = page.getByPlaceholder(COMMON.common.placeholder.search, { exact: true });

        await openFilters();
        await chosen(WORDS.brain.label.status!).click();
        await page.getByRole("option").first().waitFor();
        expect(await page.getByRole("option").count()).toBe(8);
        expect(await search.isVisible()).toBe(true);

        // The list stands at least as wide as its trigger, on one of the trigger's edges, and inside the window.
        const trigger = (await chosen(WORDS.brain.label.status!).boundingBox())!;
        const list = await openList();
        const onStart = Math.abs(list.x - trigger.x) < 1;
        const onEnd = Math.abs(list.x + list.width - (trigger.x + trigger.width)) < 1;

        expect(list.width).toBeGreaterThanOrEqual(trigger.width - 1);
        expect(onStart || onEnd).toBe(true);
        expect(list.x).toBeGreaterThanOrEqual(0);
        expect(list.x + list.width).toBeLessThanOrEqual(viewport.width);
        await page.keyboard.press("Escape");
        await search.waitFor({ state: "hidden" });

        await openFilters();
        await chosen(WORDS.brain.label.order!).click();
        await page.getByRole("option").first().waitFor();
        expect(await page.getByRole("option").count()).toBe(5);
        expect(await search.count()).toBe(0);
      }, 60_000);
    }
  });

  describe("the last night's sleep stands at the head of the memories", () => {
    beforeEach(() => {
      seed();
      rows.push({
        id: 50,
        type: "reflection",
        text: "Slept: learned three facts, merged one pair, closed two promises.",
        layerId: null,
        position: 0,
        status: "active",
        labels: { sleep: "2026-11-24" }
      });
    });

    for (const viewport of [WIDE, NARROW]) {
      test(`at ${viewport.width} wide, the card opens to the summary and opens its row`, async () => {
        await page.setViewportSize(viewport);
        await page.goto(`${origin}/brain/1`, { waitUntil: "load" });
        const card = page.locator("[data-sleep-card]");

        await card.waitFor();
        // The card reads the newest sleep first, in an order the brain accepts: the stand-in refuses any other, and
        // the card never stands over a refused read.
        const sleepRead = reads.find((read) => read.path === "/brains/1/memories" && read.query.get("label") === "sleep");
        expect(sleepRead?.query.get("order")).toBe("-validFrom");
        expect(await card.innerText()).toContain("2026-11-24");
        expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
        await card.getByRole("button").first().click();
        await card.getByText("Slept: learned three facts", { exact: false }).waitFor();
        await card.getByRole("button", { name: WORDS.brain.action.open_sleep!, exact: true }).click();
        await until(() => new URL(page.url()).searchParams.get("memory") === "50", "the sleep's row");
      }, 60_000);
    }

    test("a brain that has never slept shows no card", async () => {
      seed();
      await page.setViewportSize(WIDE);
      await page.goto(`${origin}/brain/1`, { waitUntil: "load" });
      await page.locator("h2").first().waitFor();
      await Bun.sleep(300);
      expect(await page.locator("[data-sleep-card]").count()).toBe(0);
    }, 60_000);
  });

  describe("the memories are read a page at a time as the list's end comes into view", () => {
    // Seventy memories past the eight seeded: a first page of fifty, and a second the scroll alone brings.
    beforeEach(() => {
      seed();
      for (let n = 1; n <= 70; n++) rows.push({ id: 100 + n, type: "fact", text: `paged ${n}`, layerId: null, position: 0, status: "active" });
    });

    const memory = (text: string) => page.getByText(text, { exact: true });
    const nextPages = () => reads.filter((read) => read.path === "/brains/1/memories" && read.query.get("after") !== null);

    // The wheel turned over the list until the memory stands, as a reader scrolling to the end does.
    async function scrollTo(text: string): Promise<void> {
      await until(async () => {
        await page.mouse.wheel(0, 1200);

        return (await memory(text).count()) > 0;
      }, `the memory "${text}"`);
    }

    for (const viewport of [WIDE, NARROW]) {
      test(`at ${viewport.width} wide, scrolling to the end reads the next page, and nothing is left to press`, async () => {
        await page.setViewportSize(viewport);
        await page.goto(`${origin}/brain/1/list`, { waitUntil: "load" });
        await memory("paged 1").waitFor();
        expect(nextPages()).toEqual([]);
        expect(await memory("paged 70").count()).toBe(0);
        // The pack holds no words for a button that loads more, and the foot holds no button.
        expect(WORDS.brain.action.more).toBeUndefined();
        expect(await page.locator("[data-next-page-foot] button").count()).toBe(0);
        // In the table, the last row of data draws no line under it while the foot follows it.
        if (viewport === WIDE) {
          const line = await page.evaluate(() => {
            const rows = [...document.querySelectorAll("[data-memory-table] tbody tr:not([data-next-page-foot])")];

            return getComputedStyle(rows[rows.length - 1]!).borderBottomWidth;
          });

          expect(line).toBe("0px");
        }

        const box = await memory("paged 1").first().boundingBox();

        await page.mouse.move(box!.x + box!.width / 2, box!.y + box!.height / 2);
        await scrollTo("paged 70");
        expect(nextPages().length).toBe(1);
        // The last page holds nothing under it.
        await until(async () => (await page.locator("[data-next-page-foot]").count()) === 0, "the foot gone at the end");
      }, 60_000);
    }

    test("a page that fails keeps the memories read and is read again from its retry", async () => {
      await page.setViewportSize(NARROW);
      await page.goto(`${origin}/brain/1/list`, { waitUntil: "load" });
      await memory("paged 1").waitFor();
      failNextPage = true;

      const box = await memory("paged 1").first().boundingBox();

      await page.mouse.move(box!.x + box!.width / 2, box!.y + box!.height / 2);

      const retry = page.locator("[data-next-page-foot]").getByRole("button", { name: COMMON.common.action.retry, exact: true });

      await until(async () => {
        await page.mouse.wheel(0, 1200);

        return (await retry.count()) > 0;
      }, "the failed page's retry");
      expect(await page.locator("[data-next-page-foot] [role=alert]").innerText()).toContain("unavailable");
      expect(await memory("paged 49").count()).toBe(1);
      expect(await memory("paged 50").count()).toBe(0);

      await retry.click();
      await memory("paged 70").waitFor();
      expect(nextPages().length).toBe(2);
    }, 60_000);
  });

  describe("an org brain is read here and written on the organisation screen", () => {
    beforeEach(seed);

    const said = WORDS.brain.hint.org_read_only!;
    const button = (name: string) => page.locator(`button[aria-label="${name}"]`);

    for (const viewport of [WIDE, NARROW]) {
      test(`at ${viewport.width} wide, the base stands read-only, its line and its create button saying where it is written`, async () => {
        await page.setViewportSize(viewport);
        await page.goto(`${origin}/brain/2/book`, { waitUntil: "load" });
        await piece(ORG_TEXT).waitFor();
        await page.locator("p", { hasText: said }).first().waitFor();
        expect(await button(said).isDisabled()).toBe(true);
        expect(await button(WORDS.brain.action.create!).count()).toBe(0);

        // The layer view's piece is neither carried nor moved, and the index carries and describes nothing.
        expect(await piece(ORG_TEXT).getAttribute("draggable")).toBe("false");
        expect(await button(WORDS.brain.action.up!).count()).toBe(0);
        expect(await button(WORDS.brain.action.down!).count()).toBe(0);
        if (viewport === WIDE) {
          expect(await layerRow("rules").getAttribute("draggable")).toBe("false");
          expect(await button(WORDS.brain.action.describe!).count()).toBe(0);
        }

        // The memory open holds no edit and no status move.
        await page.goto(`${origin}/brain/2/list?memory=201`, { waitUntil: "load" });
        await button(WORDS.brain.action.all_versions!).waitFor();
        expect(await button(WORDS.brain.action.edit!).count()).toBe(0);
        for (const move of ["approve", "confirm", "done", "invalidate", "retract", "restore"]) {
          expect({ move, shown: await page.getByRole("button", { name: WORDS.brain.action[move]!, exact: true }).count() }).toEqual({ move, shown: 0 });
        }
        expect(writes).toEqual([]);
      }, 60_000);
    }

    test("an agent's brain keeps its create button and says nothing of the organisation screen", async () => {
      await page.setViewportSize(WIDE);
      await page.goto(`${origin}/brain/1/list`, { waitUntil: "load" });
      await page.getByText("unfiled one").first().waitFor();
      expect(await button(WORDS.brain.action.create!).isDisabled()).toBe(false);
      expect(await page.locator("p", { hasText: said }).count()).toBe(0);
    }, 60_000);
  });
});
