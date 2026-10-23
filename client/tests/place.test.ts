import { expect, test } from "bun:test";

import { filterOf, orderOf, pathOf, placeOf, queryOf } from "../app/domain/brain-place";

test("a plain address is one brain's live memories but episodes, newest first", () => {
  const place = placeOf(["7"], {});

  expect(place.brainId).toBe(7);
  expect(place.view).toBe("list");
  expect(filterOf(place)).toEqual({ status: "active,pending", excludeTypes: "episode" });
  expect(orderOf(place)).toBe("-validFrom");
  expect(pathOf(place)).toBe("/brain/7");
  expect(queryOf(place)).toEqual({});
});

test("a place written to an address reads back the same", () => {
  const place = placeOf(["3", "map"], { status: "all", type: "*", layer: "unfiled", subject: "9", q: "tea", memory: "41" });

  expect(placeOf(pathOf(place).split("/").slice(2), queryOf(place) as Record<string, string>)).toEqual(place);
  expect(filterOf(place)).toEqual({ unfiled: true, subjectId: 9, q: "tea" });
});

test("a layer's memories stand by position, and a count leaves its own filter out", () => {
  const place = placeOf(["1"], { layer: "4", status: "done", type: "fact" });

  expect(orderOf(place)).toBe("position");
  expect(filterOf(place, "status")).toEqual({ type: "fact", layerId: 4 });
  expect(filterOf(place, "type")).toEqual({ status: "done", layerId: 4 });
});

test("an address that names nothing known falls back to the defaults", () => {
  const place = placeOf(["x", "nowhere"], { status: "asleep", order: "text" });

  expect(place.brainId).toBeNull();
  expect(place.view).toBe("list");
  expect(place.status).toBe("live");
  expect(place.order).toBe("-validFrom");
});
