// The committed types are what openapi-typescript makes of docs/openapi.json, and the package carries the
// document's version, so neither the client nor its manifest drifts from the contract: `bun run generate`
// writes the types again.

import { expect, test } from "bun:test";
import openapiTS, { astToString, COMMENT_HEADER } from "openapi-typescript";

import document from "../../docs/openapi.json";
import manifest from "../package.json";

test("the committed types are the generated ones", async () => {
  const source = new URL("../../docs/openapi.json", import.meta.url);
  const generated = COMMENT_HEADER + astToString(await openapiTS(source));
  const committed = await Bun.file(new URL("../openapi.d.ts", import.meta.url)).text();

  expect(committed).toBe(generated);
});

test("the package carries the document's version", () => {
  expect(manifest.version).toBe(document.info.version);
});
