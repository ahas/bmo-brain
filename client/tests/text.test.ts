import { plugin } from "bun";
import { expect, mock, test } from "bun:test";
import { readFileSync } from "node:fs";

import { compileScript, parse } from "@vue/compiler-sfc";
import { renderToString } from "@vue/server-renderer";
import { computed, createSSRApp, defineComponent, h, ref, watch } from "vue";
import { createI18n, useI18n } from "vue-i18n";

import words from "../i18n/locales/ko.json";

import type { Reading } from "bmo-ui/app/domain/translation";

// A text of the brain as the screen reads it (docs/design.md, 「The screen」), rendered the way the console renders
// it: a text the translator stayed too busy to read stands as its English, with no label and no original to open.

// The translator: a text starting "Busy" comes back untranslated, any other is read whole.
mock.module("bmo-ui/app/domain/translation", () => ({
  TRANSLATION_BATCH: 4,
  hasTranslator: () => true,
  translated: async (texts: readonly string[], language: string): Promise<Reading[]> =>
    texts.map((text) => (text.startsWith("Busy") ? { text, whole: false, untranslated: true } : { text: `${language}:${text}`, whole: true }))
}));

// A single-file component is compiled as the bundle's build compiles it, its auto-imports standing as globals.
plugin({
  name: "vue",
  setup(build) {
    build.onLoad({ filter: /\.vue$/ }, ({ path }) => {
      const { descriptor } = parse(readFileSync(path, "utf8"), { filename: path });
      const script = compileScript(descriptor, { id: path, inlineTemplate: true, templateOptions: { ssr: true } });

      return { contents: script.content, loader: "ts" };
    });
  }
});

Object.assign(globalThis, { computed, ref, watch, useI18n });

const { default: BrainText } = await import("../app/components/BrainText.vue");
const { useReading } = await import("../app/domain/brain-reading");

const i18n = createI18n({ legacy: false, locale: "ko", messages: { ko: words } });

const UIcon = defineComponent({ render: () => null });
const UButton = defineComponent({ props: { label: String }, setup: (props) => () => h("button", props.label) });

async function rendered(text: string): Promise<string> {
  const app = createSSRApp(() => h(BrainText, { text }));

  app.use(i18n);
  app.component("UIcon", UIcon);
  app.component("UButton", UButton);

  return renderToString(app);
}

const settled = () => new Promise((done) => setTimeout(done, 5));

test("a text read whole shows its reading, says it is translated and opens its original", async () => {
  await rendered("Likes tea");
  await settled();

  const html = await rendered("Likes tea");

  expect(html).toContain("ko:Likes tea");
  expect(html).toContain(words.brain.label.translated);
  expect(html).toContain(words.brain.action.show_original);
  // The line saying so stands above the text, not under it.
  expect(html.indexOf(words.brain.label.translated)).toBeLessThan(html.indexOf("ko:Likes tea"));
});

test("an untranslated text shows its English with no translated label and no original link, and the screen says so", async () => {
  await rendered("Busy keeping a garden");
  await settled();

  const html = await rendered("Busy keeping a garden");

  expect(html).toContain("Busy keeping a garden");
  expect(html).not.toContain(words.brain.label.translated);
  expect(html).not.toContain(words.brain.label.translated_in_part);
  expect(html).not.toContain(words.brain.action.show_original);
  expect(html).not.toContain(words.brain.status.translating);

  // The screen's one reader, which the component took, holds the failure the page says in its one sentence.
  expect(useReading().failed.value).toBe(true);
});
