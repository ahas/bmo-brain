import { expect, test } from "bun:test";

import { carriedTo } from "../app/domain/brain-order";

const spaced = [
  { id: 1, position: 10 },
  { id: 2, position: 20 },
  { id: 3, position: 30 },
  { id: 4, position: 40 }
];
const packed = [
  { id: 1, position: 1 },
  { id: 2, position: 2 },
  { id: 3, position: 3 },
  { id: 4, position: 4 }
];

test("a piece put down where positions leave room is the one piece written", () => {
  expect(carriedTo(spaced, 3, 1)).toEqual([{ id: 4, position: 15 }]);
  expect(carriedTo(spaced, 0, 4)).toEqual([{ id: 1, position: 41 }]);
  expect(carriedTo(spaced, 2, 0)).toEqual([{ id: 3, position: 9 }]);
});

test("a piece put down between neighbours with no room moves on the ones after it only as far as they must", () => {
  expect(carriedTo(packed, 3, 1)).toEqual([
    { id: 4, position: 2 },
    { id: 2, position: 3 },
    { id: 3, position: 4 }
  ]);
  expect(carriedTo(packed, 0, 3)).toEqual([
    { id: 1, position: 4 },
    { id: 4, position: 5 }
  ]);
});

test("a piece put down beside itself, or nowhere among the pieces, writes nothing", () => {
  expect(carriedTo(packed, 1, 1)).toEqual([]);
  expect(carriedTo(packed, 1, 2)).toEqual([]);
  expect(carriedTo(packed, 1, 9)).toEqual([]);
  expect(carriedTo([{ id: 1, position: 1 }], 0, 1)).toEqual([]);
});
