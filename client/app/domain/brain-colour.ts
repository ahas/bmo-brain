// The colour of a group — a layer or a type — the same wherever it stands on the screen: one of a few hues
// chosen by the group's name, so a colour follows the name and not the order a list happens to read in.

const HUES = [210, 150, 30, 280, 350, 90, 180, 250, 60, 320];

export function groupColour(name: string): string {
  let hash = 0;

  for (const character of name) hash = (hash * 31 + character.codePointAt(0)!) >>> 0;

  return `hsl(${HUES[hash % HUES.length]} 62% 52%)`;
}
