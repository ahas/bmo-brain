import { fileURLToPath } from "node:url";

// The brain's screen stood alone for the browser tests: the client's layer, which carries bmo-ui's, with a page
// shell of its own in place of a console's. What the host builds and caches stands under the client's packages,
// where it resolves them and no checkout file sees it.
const cache = fileURLToPath(new URL("../../../node_modules/.cache/brain-browser-host", import.meta.url));

export default defineNuxtConfig({
  extends: ["../../.."],
  buildDir: `${cache}/nuxt`,
  vite: { cacheDir: `${cache}/vite` },
  devtools: { enabled: false },
  telemetry: false
});
