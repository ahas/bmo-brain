import { TRANSLATION_BATCH, hasTranslator, translated } from "bmo-ui/app/domain/translation";
import { ref, shallowReactive, watch } from "vue";
import { useI18n } from "vue-i18n";

import { holdsHangul } from "./brain-words";

import type { Reading } from "bmo-ui/app/domain/translation";
import type { Ref } from "vue";

// The reading language and the writing language of the screen (docs/design.md, 「The screen」). The brain's rows
// and its persona are English; the person at the screen may read the console's own language instead, each text
// translated through the translator the console names as it is shown, and the English one tap away. Words typed
// in the console's language are read into English before they are saved, so no row holds another script.

/** The language the brain's rows are written in. */
export const ENGLISH = "en";

type Translate = (texts: readonly string[], language: string) => Promise<Reading[]>;

/** What the screen reads its texts through. */
export interface Reader {
  // The console's own language, which the screen translates into where a translator is named.
  home: string;
  // Whether a translation is offered at all: a translator is named and the console's language is not English.
  offered: boolean;
  // The language the texts are read in: English or the console's own.
  language: Ref<string>;
  // Whether a translation was asked for and did not come, or came back untranslated, which the screen says in one
  // sentence.
  failed: Ref<boolean>;
  // Asks for the readings of texts in the reading language; the English stands until each comes.
  want: (texts: readonly string[]) => void;
  // The reading of a text in the reading language, where it has come; none while reading English.
  of: (text: string) => Reading | undefined;
  // The text as the screen shows it: its reading where one has come, the English otherwise.
  shown: (text: string) => string;
  // Whether a text's reading is on its way: asked for in a language other than English, neither come nor failed.
  pending: (text: string) => boolean;
  // Reads in a language, asking again for what failed before.
  choose: (language: string) => void;
}

// How many asks for a page's texts stand at once.
const ASKS_AT_ONCE = 2;

/** A reader of texts in the console's language through a translation, starting in that language where offered. */
export function readerOver(home: string, translate: Translate, translator: boolean): Reader {
  const offered = translator && home !== ENGLISH;
  const language = ref(offered ? home : ENGLISH);
  const failed = ref(false);
  const shelf = shallowReactive(new Map<string, Reading>());
  const missed = shallowReactive(new Set<string>());
  let waiting = new Set<string>();
  let timer: ReturnType<typeof setTimeout> | null = null;

  const keyOf = (tag: string, text: string): string => `${tag}\u0000${text}`;

  // Each batch is put on the shelf as it comes, so a page's first texts stand translated before its last. A text the
  // translator stayed too busy to read comes back untranslated and is missed as a failed one is: the English stands
  // with the one sentence, and choosing the language again asks for it again.
  async function flush(): Promise<void> {
    timer = null;

    const tag = language.value;
    const texts = [...waiting];

    waiting = new Set();

    if (tag === ENGLISH || texts.length === 0) return;

    const batches: string[][] = [];

    for (let at = 0; at < texts.length; at += TRANSLATION_BATCH) batches.push(texts.slice(at, at + TRANSLATION_BATCH));

    const lanes = Array.from({ length: Math.min(ASKS_AT_ONCE, batches.length) }, async () => {
      for (let batch = batches.shift(); batch !== undefined; batch = batches.shift()) {
        try {
          const readings = await translate(batch, tag);

          batch.forEach((text, at) => {
            const reading = readings[at]!;

            if (reading.untranslated) {
              failed.value = true;
              missed.add(keyOf(tag, text));
            } else shelf.set(keyOf(tag, text), reading);
          });
        } catch {
          failed.value = true;
          for (const text of batch) missed.add(keyOf(tag, text));
        }
      }
    });

    await Promise.all(lanes);
  }

  function want(texts: readonly string[]): void {
    const tag = language.value;

    if (tag === ENGLISH) return;

    for (const text of texts) {
      const key = keyOf(tag, text);

      if (text.trim() !== "" && !shelf.has(key) && !missed.has(key)) waiting.add(text);
    }

    if (waiting.size > 0 && timer === null) timer = setTimeout(() => void flush(), 0);
  }

  function of(text: string): Reading | undefined {
    return language.value === ENGLISH ? undefined : shelf.get(keyOf(language.value, text));
  }

  return {
    home,
    offered,
    language,
    failed,
    want,
    of,
    shown: (text) => of(text)?.text ?? text,
    pending: (text) => language.value !== ENGLISH && text.trim() !== "" && of(text) === undefined && !missed.has(keyOf(language.value, text)),
    choose(next) {
      language.value = next;
      failed.value = false;
      missed.clear();
    }
  };
}

let reader: Reader | null = null;

/** The screen's one reader, in the console's own language where the console names a translator. */
export function useReading(): Reader {
  if (reader === null) {
    const { locale } = useI18n();

    reader = readerOver(String(locale.value).split("-")[0] ?? ENGLISH, translated, hasTranslator());
  }

  return reader;
}

/**
 * Why words could not be made English for a row: no translator, a translation that failed or came back
 * untranslated, or Hangul still in it.
 */
export type EnglishFault = "no-translator" | "failed" | "not-english";

/** The key, in the screen's words, of the one sentence that says why words were not saved. */
export function faultKey(fault: EnglishFault): string {
  return fault === "no-translator" ? "brain.error.no_translator" : fault === "failed" ? "brain.error.translate_write" : "brain.error.english_only";
}

/**
 * Words for a row as the screen saves them: words already English stand; words in another script are first read
 * into English and shown to be confirmed, so the first call answers null and leaves the English in `english`, and
 * the call after the person confirms answers the English, as they left it.
 */
export function useEnglishWords(words: () => string) {
  const english = ref<string | null>(null);
  const fault = ref<EnglishFault | null>(null);
  const busy = ref(false);

  watch(words, () => {
    english.value = null;
    fault.value = null;
  });

  async function ready(translate: Translate = translated, translator = hasTranslator()): Promise<string | null> {
    fault.value = null;

    if (english.value !== null) {
      const confirmed = english.value.trim();

      if (holdsHangul(confirmed)) {
        fault.value = "not-english";
        return null;
      }

      return confirmed;
    }

    const typed = words().trim();

    if (!holdsHangul(typed)) return typed;

    if (!translator) {
      fault.value = "no-translator";
      return null;
    }

    busy.value = true;

    try {
      const [reading] = await translate([typed], ENGLISH);

      if (reading?.untranslated) fault.value = "failed";
      else english.value = reading?.text ?? typed;
    } catch {
      fault.value = "failed";
    } finally {
      busy.value = false;
    }

    return null;
  }

  return { english, fault, busy, ready };
}
