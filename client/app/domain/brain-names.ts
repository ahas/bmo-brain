// The names a type, a subject's type and a status are shown by (docs/design.md, 「The screen」, "Names in the
// reader's language"): the language pack's, and a type the pack does not name by its own word, so no code or enum
// word stands on the screen where the pack has a name for it.

type Translate = (key: string) => string;
type Knows = (key: string) => boolean;
type Messages = (key: string) => unknown;

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

// The names the reader's language gives the types, as the `type=name` pairs the brain's type search takes: a
// type whose name is its own word needs none, since the search matches that word already. The screen sends them
// with the words typed, so a type is found by the name it shows and the match is marked on that name.
export function typeSearchNames(t: Translate, tm: Messages, te: Knows): string {
  const table = tm("brain.type_name");

  if (typeof table !== "object" || table === null) return "";

  return Object.keys(table)
    .filter((type) => te(`brain.type_name.${type}`))
    .flatMap((type) => {
      const name = t(`brain.type_name.${type}`);

      return name.toLocaleLowerCase() === type.toLocaleLowerCase() ? [] : [`${type}=${name}`];
    })
    .join(",");
}
