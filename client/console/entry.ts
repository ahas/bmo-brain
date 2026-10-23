import type { ConsoleBundleModule } from "bmo-ui/shared/console-manifest";

import "./style.css";

// The module's default export (bmo-ui's `docs/console-bundles.md`, "What a bundle is"): the brain screen's
// one route under its screen path, `/brain`. Every place on the screen is the same page, keyed once, so a
// memory opened or a filter changed never builds the screen anew.
const bundle: ConsoleBundleModule = {
  routes: [
    {
      path: "/brain/:path(.*)*",
      name: "brain-path",
      meta: { key: () => "brain" },
      component: () => import("../app/pages/brain/[...path].vue")
    }
  ]
};

export default bundle;
