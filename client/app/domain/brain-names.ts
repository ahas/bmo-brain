// The names a type, a subject's type and a status are shown by (docs/design.md, 「The screen」, "Names in the
// reader's language"): the language pack's, and a type the pack does not name by its own word, so no code or enum
// word stands on the screen where the pack has a name for it.

type Translate = (key: string) => string;
type Knows = (key: string) => boolean;

function named(t: Translate, te: Knows, key: string, fallback: string): string {
  return te(key) ? t(key) : fallback;
}

// A memory's type by its name.
export function typeName(t: Translate, te: Knows, type: string): string {
  return named(t, te, `brain.type_name.${type}`, type);
}

// A subject's type by its name.
export function subjectTypeName(t: Translate, te: Knows, type: string): string {
  return named(t, te, `brain.subject_type.${type}`, type);
}

// The types the packs name, each with the whole key its name stands under. They are written one by one because the
// console bundle's word check reads every whole key a screen asks and holds every pack to answering it as a word,
// while the namespace's own key stands for a tree, which no pack answers as a word.
const TYPE_NAME_KEYS: readonly (readonly [type: string, key: string])[] = [
  ["episode", "brain.type_name.episode"],
  ["fact", "brain.type_name.fact"],
  ["rumor", "brain.type_name.rumor"],
  ["situation", "brain.type_name.situation"],
  ["rule", "brain.type_name.rule"],
  ["name", "brain.type_name.name"],
  ["persona", "brain.type_name.persona"],
  ["lore", "brain.type_name.lore"],
  ["knowledge", "brain.type_name.knowledge"],
  ["check", "brain.type_name.check"],
  ["followup", "brain.type_name.followup"],
  ["promise", "brain.type_name.promise"],
  ["state", "brain.type_name.state"],
  ["tip", "brain.type_name.tip"]
];

// The names the reader's language gives the types, as the `type=name` pairs the brain's type search takes: a
// type whose name is its own word needs none, since the search matches that word already. The screen sends them
// with the words typed, so a type is found by the name it shows and the match is marked on that name.
export function typeSearchNames(t: Translate, te: Knows): string {
  return TYPE_NAME_KEYS.flatMap(([type, key]) => {
    if (!te(key)) return [];

    const name = t(key);

    return name.toLocaleLowerCase() === type.toLocaleLowerCase() ? [] : [`${type}=${name}`];
  }).join(",");
}
