// What the screen reads off a memory's words: the change from one version to the next, and whether words
// are English enough for the brain, which keeps its memories in English (docs/design.md, 「Writing」).

export interface Piece {
  text: string;
  change: "same" | "added" | "removed";
}

// The words of `before` and `after` as one run, each word kept, added or removed: the longest common run of
// words stands, and what is outside it is the change.
export function wordChange(before: string, after: string): Piece[] {
  const a = before.split(/(\s+)/).filter((word) => word !== "");
  const b = after.split(/(\s+)/).filter((word) => word !== "");
  const lengths: number[][] = Array.from({ length: a.length + 1 }, () => new Array<number>(b.length + 1).fill(0));

  for (let i = a.length - 1; i >= 0; i--) {
    for (let j = b.length - 1; j >= 0; j--) {
      lengths[i]![j] = a[i] === b[j] ? lengths[i + 1]![j + 1]! + 1 : Math.max(lengths[i + 1]![j]!, lengths[i]![j + 1]!);
    }
  }

  const pieces: Piece[] = [];
  const put = (text: string, change: Piece["change"]): void => {
    const last = pieces[pieces.length - 1];

    if (last !== undefined && last.change === change) last.text += text;
    else pieces.push({ text, change });
  };
  let i = 0;
  let j = 0;

  while (i < a.length && j < b.length) {
    if (a[i] === b[j]) {
      put(a[i]!, "same");
      i++;
      j++;
    } else if (lengths[i + 1]![j]! >= lengths[i]![j + 1]!) {
      put(a[i++]!, "removed");
    } else {
      put(b[j++]!, "added");
    }
  }
  while (i < a.length) put(a[i++]!, "removed");
  while (j < b.length) put(b[j++]!, "added");

  return pieces;
}

// Whether words hold a script of the screen's other languages, Hangul, kana or Han, which the screen reads into
// English before it saves them and does not save while no translator reaches it.
export function holdsOtherScript(words: string): boolean {
  return /[\p{Script=Hangul}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Han}]/u.test(words);
}
