import { fileURLToPath } from "node:url";

// The brain's screen stood alone for the browser tests: the client's layer, which carries bmo-ui's, with a page
// shell of its own in place of a console's. What the host builds and caches stands under the client's packages,
// where it resolves them and no checkout file sees it.
const cache = fileURLToPath(new URL("../../../node_modules/.cache/brain-browser-host", import.meta.url));

export default defineNuxtConfig({
  extends: ["../../.."],
  // The layer's `common.*` words the screen draws, which a console answers from its own pack and the bundle's
  // words never hold: merged here into the screen's Korean, so a browser test reads the words a console shows.
  i18n: {
    locales: [{ code: "ko", language: "ko-KR", file: "ko.json" }]
  },
  buildDir: `${cache}/nuxt`,
  vite: { cacheDir: `${cache}/vite` },
  devtools: { enabled: false },
  telemetry: false
});
