// Where a piece of a layer carried to another place in the layer view stands, and which pieces are written again
// for it (docs/design.md, 「The screen」). Every write is a new version, so as few pieces are written as the order
// allows: the carried one alone where the positions about its landing leave room, and the ones after it moved on
// only as far as they must.

// The drag type a piece of the layer view travels under, its id and type in JSON, so the index's layers take it.
export const PIECE_DRAG = "application/x-brain-piece";

export interface Carried {
  id: number;
  type: string;
  layerId: number | null;
}

export interface Placed {
  id: number;
  position: number;
}

/**
 * The positions written when the piece at `from`, among pieces in position order, is put down before the piece at
 * `land` (their count, past the last); none where it would stand where it stood.
 */
export function carriedTo(pieces: readonly Placed[], from: number, land: number): Placed[] {
  const moving = pieces[from];

  if (moving === undefined || land < 0 || land > pieces.length) return [];

  const to = land > from ? land - 1 : land;

  if (to === from) return [];

  const others = pieces.filter((_, index) => index !== from);
  const before = others[to - 1];
  const after = others[to];

  if (before === undefined) return after === undefined ? [] : [{ id: moving.id, position: after.position - 1 }];
  if (after === undefined) return [{ id: moving.id, position: before.position + 1 }];
  if (after.position - before.position >= 2) return [{ id: moving.id, position: before.position + Math.floor((after.position - before.position) / 2) }];

  const written: Placed[] = [{ id: moving.id, position: before.position + 1 }];
  let last = before.position + 1;

  for (const piece of others.slice(to)) {
    if (piece.position > last) break;

    last += 1;
    written.push({ id: piece.id, position: last });
  }

  return written;
}
