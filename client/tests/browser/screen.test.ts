import { afterAll, beforeAll, beforeEach, describe, expect, test } from "bun:test";
import { spawn } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";

import { chromium } from "playwright-core";

import type { ChildProcess } from "node:child_process";
import type { Browser, Locator, Page, Route } from "playwright-core";

// The brain's screen (docs/design.md, 「The screen」) driven in a real Chromium. The pointer's three carries, with
// the mouse: a piece of the layer view carried to another place, a layer carried among the layers, and a piece
// dropped on another layer, on the unfiled or on its own layer. And its selects, at a wide window and a phone's: a
// list of more than seven choices opens with a search field, one of seven or fewer without. The screen stands alone
// on the host under `host/`, and the brain's routes are answered by a stand-in holding one brain, which keeps every
// write, so what the screen wrote is read back and the screen read again shows it. Skipped where the browser
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
  brain: { title: Record<string, string>; label: Record<string, string>; action: Record<string, string> };
};
// The layer's words the host answers beside the screen's own (`host/i18n/`).
const COMMON = JSON.parse(readFileSync(new URL("./host/i18n/locales/ko.json", import.meta.url), "utf8")) as {
  common: { placeholder: { search: string } };
};

// --- The stand-in brain ---

interface Row {
  id: number;
  type: string;
  text: string;
  layerId: number | null;
  position: number;
  status: "active" | "superseded";
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

const AT = 1_790_000_000_000;
let layers: LayerRow[] = [];
let rows: Row[] = [];
let writes: Write[] = [];

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
    labels: {},
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

function listed(query: URLSearchParams): Row[] {
  const statuses = query.get("status")?.split(",");
  const types = query.get("type")?.split(",");
  const layerId = query.get("layerId");

  return rows
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

  switch (path) {
    case "/events":
      // A stream that names an hour to wait before it is opened again, so the page holds no stream and reads afresh
      // only when the test loads it.
      return route.fulfill({ status: 200, contentType: "text/event-stream", body: "retry: 3600000\n\n" });
    case "/brains":
      return json(route, { items: [{ id: 1, accountKey: "account-a", agentKey: "writer-a", name: "brain-a", createdAt: AT }], more: false });
    case "/brains/1/layers":
      return json(route, { items: [...layers].sort(byPosition).map(layerOf), more: false });
    case "/brains/1/subjects":
      return json(route, { items: [], more: false });
    case "/brains/1/types":
      return json(route, [...new Set(rows.map((row) => row.type))].map((type) => ({ type, reserved: type === "persona", count: 1 })));
    case "/brains/1/tallies":
      return json(route, []);
    case "/brains/1/memories": {
      const items = listed(url.searchParams);

      return json(route, { items: items.slice(0, Number(url.searchParams.get("limit") ?? "50")).map(memoryOf), more: false });
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
    // Run by Bun itself, in its own process group, so the host and everything it starts go down together; what it
    // says is kept for the message of a start that fails.
    host = spawn(process.execPath, ["--bun", `${CLIENT}node_modules/.bin/nuxi`, "dev", HOST, "--host", "127.0.0.1", "--port", String(port)], {
      cwd: CLIENT,
      detached: true,
      stdio: ["ignore", "pipe", "pipe"],
      env: { ...process.env, NUXT_TELEMETRY_DISABLED: "1" }
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
});
