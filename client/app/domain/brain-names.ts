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
