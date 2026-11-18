import { expect, test } from "bun:test";
import { Glob } from "bun";

// Every value the screen chooses from a list is chosen in bmo-ui's one select (docs/design.md, 「The screen」),
// so the layer alone decides whether a list searches; read off the files the bundle ships.

const APP = new URL("../app/", import.meta.url).pathname;
const BARE = /<(USelect|USelectMenu|UInputMenu)\b/;

test("no screen stands the library's select, select menu or input menu bare", async () => {
  const found: string[] = [];

  for await (const file of new Glob("**/*.vue").scan(APP)) {
    if (BARE.test(await Bun.file(APP + file).text())) found.push(file);
  }

  expect(found).toEqual([]);
});
