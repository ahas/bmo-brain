import { expect, test } from "bun:test";
import { nextTick, ref } from "vue";

import { ENGLISH, readerOver, useEnglishWords } from "../app/domain/brain-reading";

import type { Reading } from "bmo-ui/app/domain/translation";

// The reading language and the writing language of the screen (docs/design.md, 「The screen」).

const asked: { texts: readonly string[]; language: string }[] = [];

async function marking(texts: readonly string[], language: string): Promise<Reading[]> {
  asked.push({ texts, language });

  return texts.map((text) => ({ text: `${language}:${text}`, whole: true }));
}

async function refusing(): Promise<Reading[]> {
  throw new Error("the translator did not answer");
}

const settled = () => new Promise((done) => setTimeout(done, 5));

test("a reader starts in the console's language where a translator is named, and in English otherwise", () => {
  expect(readerOver("ko", marking, true).language.value).toBe("ko");
  expect(readerOver("ko", marking, false).language.value).toBe(ENGLISH);
  expect(readerOver("ko", marking, false).offered).toBe(false);
  expect(readerOver(ENGLISH, marking, true).offered).toBe(false);
});

test("the English stands while a reading is on its way, and the reading once it has come", async () => {
  const reader = readerOver("ko", marking, true);

  reader.want(["Likes tea", "Likes tea", ""]);

  expect(reader.shown("Likes tea")).toBe("Likes tea");
  expect(reader.pending("Likes tea")).toBe(true);

  await settled();

  expect(reader.shown("Likes tea")).toBe("ko:Likes tea");
  expect(reader.of("Likes tea")).toEqual({ text: "ko:Likes tea", whole: true });
  expect(reader.pending("Likes tea")).toBe(false);
  expect(asked.at(-1)).toEqual({ texts: ["Likes tea"], language: "ko" });
});

test("a reader in English asks nothing and shows the English", async () => {
  const reader = readerOver("ko", marking, true);
  const before = asked.length;

  reader.choose(ENGLISH);
  reader.want(["Walks at dawn"]);
  await settled();

  expect(asked.length).toBe(before);
  expect(reader.shown("Walks at dawn")).toBe("Walks at dawn");
  expect(reader.pending("Walks at dawn")).toBe(false);
});

test("a failed translation leaves the English, says so, and is asked again once the language is chosen again", async () => {
  const reader = readerOver("ko", refusing, true);

  reader.want(["Keeps a garden"]);
  await settled();

  expect(reader.failed.value).toBe(true);
  expect(reader.shown("Keeps a garden")).toBe("Keeps a garden");
  expect(reader.pending("Keeps a garden")).toBe(false);

  reader.choose("ko");

  expect(reader.failed.value).toBe(false);
  expect(reader.pending("Keeps a garden")).toBe(true);
});

test("English words are saved as they are typed", async () => {
  const typed = ref("Prefers green tea");
  const words = useEnglishWords(() => typed.value);

  expect(await words.ready(marking, true)).toBe("Prefers green tea");
  expect(words.english.value).toBeNull();
});

test("words in another script are shown in English first and saved as the English once confirmed", async () => {
  const typed = ref("녹차를 좋아함");
  const words = useEnglishWords(() => typed.value);

  expect(await words.ready(marking, true)).toBeNull();
  expect(words.english.value).toBe(`${ENGLISH}:녹차를 좋아함`);

  words.english.value = "Prefers green tea";

  expect(await words.ready(marking, true)).toBe("Prefers green tea");
});

test("no word of another script is saved, whatever the translation left", async () => {
  const typed = ref("녹차");
  const words = useEnglishWords(() => typed.value);

  await words.ready(marking, true);

  expect(await words.ready(marking, true)).toBeNull();
  expect(words.fault.value).toBe("not-english");
});

test("words in another script are not saved where no translator is named or the translation fails", async () => {
  const typed = ref("녹차");
  const words = useEnglishWords(() => typed.value);

  expect(await words.ready(marking, false)).toBeNull();
  expect(words.fault.value).toBe("no-translator");

  expect(await words.ready(refusing, true)).toBeNull();
  expect(words.fault.value).toBe("failed");
  expect(words.english.value).toBeNull();
});

test("the English shown is let go once the typed words change", async () => {
  const typed = ref("녹차");
  const words = useEnglishWords(() => typed.value);

  await words.ready(marking, true);
  typed.value = "Green tea";
  await nextTick();

  expect(words.english.value).toBeNull();
});
