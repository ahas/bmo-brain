// The package packs as the manifest names it: the manifest and the files its exports point at, so a
// downstream checkout installs the client from the tarball.

import { expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

test("the package packs the files its exports name", () => {
  const destination = mkdtempSync(join(tmpdir(), "brain-client-"));

  try {
    const packed = Bun.spawnSync(
      ["bun", "pm", "pack", "--destination", destination, "--ignore-scripts", "--quiet"],
      { cwd: join(import.meta.dir, ".."), stderr: "inherit" },
    );
    expect(packed.exitCode).toBe(0);

    const named = packed.stdout.toString().trim();
    const archive = named.startsWith("/") ? named : join(destination, named);
    const listed = Bun.spawnSync(["tar", "-tzf", archive], { stderr: "inherit" });
    expect(listed.exitCode).toBe(0);

    const entries = listed.stdout.toString().trim().split("\n").sort();
    expect(entries).toEqual(["package/index.ts", "package/openapi.d.ts", "package/package.json"]);
  } finally {
    rmSync(destination, { recursive: true, force: true });
  }
});
