// How a memory's standing reads on the screen (docs/design.md, 「Strength and decay」 and 「The screen」): its
// activation as a number, and its stability as the psychological days it lasts. A memory with no trace has no
// activation, and one that never falls below the threshold lasts without end.

type Translate = (key: string, values?: Record<string, unknown>) => string;

/** The activation as the screen writes it: two decimals, or a dash where there is none. */
export function activationText(activation: number | null | undefined): string {
  return activation === null || activation === undefined ? "–" : activation.toFixed(2);
}

/** The stability as the screen writes it: the days it lasts, or the word for lasting where it never falls. */
export function stabilityText(t: Translate, stability: number | null | undefined): string {
  if (stability === null || stability === undefined) return t("brain.label.lasting");

  const days = stability >= 10 ? stability.toFixed(0) : stability.toFixed(1);

  return t("brain.label.stability_days", { days });
}
