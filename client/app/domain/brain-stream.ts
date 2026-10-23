import { brainAddress } from "./brain-api";

// The brain's event stream as the screen reads it, narrowed to the brain it shows (docs/boundary-api.md,
// 「Events」), through the console's relay. A burst of events is gathered into one call of `onChange`; a
// stream the brain refused is opened afresh after a pause, and `onChange` is called then too, since what
// happened in between is not known.

const KINDS = ["memory.written", "memory.moved", "layer.changed", "subject.created", "subject.changed"];
const GATHER_MS = 400;
const REOPEN_MS = 3_000;

export function useBrainStream(brainId: () => number | null, onChange: () => void): void {
  let source: EventSource | null = null;
  let gathering: ReturnType<typeof setTimeout> | null = null;
  let reopening: ReturnType<typeof setTimeout> | null = null;
  let standing = false;

  function changed(): void {
    if (gathering !== null) return;

    gathering = setTimeout(() => {
      gathering = null;
      onChange();
    }, GATHER_MS);
  }

  function close(): void {
    source?.close();
    source = null;

    if (reopening !== null) clearTimeout(reopening);

    reopening = null;
  }

  function open(): void {
    close();

    const id = brainId();

    if (!standing || id === null) return;

    source = new EventSource(brainAddress(`/events?scope=brain:${id}&kinds=${KINDS.join(",")}`));

    for (const kind of KINDS) source.addEventListener(kind, changed);

    source.onerror = () => {
      if (source?.readyState !== EventSource.CLOSED) return;

      source = null;
      reopening = setTimeout(() => {
        reopening = null;
        open();
        changed();
      }, REOPEN_MS);
    };
  }

  onMounted(() => {
    standing = true;
    open();
  });

  watch(brainId, open);

  onBeforeUnmount(() => {
    standing = false;
    close();

    if (gathering !== null) clearTimeout(gathering);
  });
}
