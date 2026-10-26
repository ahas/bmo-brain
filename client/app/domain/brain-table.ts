// The memory table's columns (docs/design.md, 「The screen」): the text takes the room left from a floor of its own,
// and every other column one narrow width down the whole table. Each width is held twice — as the rem it stands
// for, and as the literal class the stylesheet is built from — so the two are read together and never part.

export type MemoryColumnId = "type" | "text" | "layer" | "status" | "importance" | "validFrom";

export interface MemoryColumn {
  id: MemoryColumnId;
  // The column's width in rem: the one width it keeps, or for the flexible column the least it takes.
  rem: number;
  flexible: boolean;
  // The classes its name and its cells stand under.
  th: string;
  td: string;
}

export const MEMORY_COLUMNS: readonly MemoryColumn[] = [
  { id: "type", rem: 5, flexible: false, th: "w-20", td: "max-w-20 text-muted" },
  { id: "text", rem: 8, flexible: true, th: "w-full min-w-32", td: "w-full max-w-0" },
  { id: "layer", rem: 6, flexible: false, th: "w-24", td: "max-w-24 text-muted" },
  { id: "status", rem: 4.5, flexible: false, th: "w-18", td: "max-w-18" },
  { id: "importance", rem: 3, flexible: false, th: "w-12 text-end", td: "max-w-12 text-end tabular-nums" },
  { id: "validFrom", rem: 5.5, flexible: false, th: "w-22 text-end", td: "max-w-22 text-end text-xs text-muted tabular-nums" }
];

// Under this width of its pane the table gives way to the stacked rows a phone shows.
export const STACK_BELOW_PX = 588;

// The least width the table's columns take together, in pixels.
export function leastTableWidth(columns: readonly MemoryColumn[] = MEMORY_COLUMNS): number {
  return columns.reduce((sum, column) => sum + column.rem * 16, 0);
}
