<template>
  <!-- The layer view (docs/design.md, 「The screen」): the layer chosen in the index, the first where none is, its
       items in position order narrowed by the type, the subject, the status and the words, moved up and down as
       new versions. A layer holding persona items shows its prompt source, the very bytes the brain hands a
       system prompt, which no filter narrows; a layer is exported as a brain file. -->
  <div class="flex min-h-0 flex-1 flex-col">
    <BrainHeading v-bind="heading" :title="title" :count="current === undefined && layer !== 'unfiled' ? null : pieces.length" :notes="notes">
      <template #tools>
        <TabSwitch v-if="hasPersona" v-model="reading" :items="readings" :label="t('brain.label.reading')" />
        <UTooltip v-if="current" :text="t('brain.action.export')">
          <UButton size="sm" square icon="i-material-symbols-download" color="neutral" variant="ghost" :aria-label="t('brain.action.export')" :to="exportHref" external download />
        </UTooltip>
      </template>
    </BrainHeading>

    <p v-if="current === undefined && layer !== 'unfiled'" class="p-6 text-center text-sm text-muted">{{ t("brain.status.no_layers") }}</p>
    <div v-else ref="scroller" class="min-h-0 flex-1 overflow-y-auto pb-(--bmo-create-room)">
      <div v-if="current?.description" class="border-b border-default px-4 py-2 text-sm text-muted">{{ current.description }}</div>

      <BrainText v-if="reading === 'rendering' && hasPersona" :text="rendering" tag="pre" text-class="font-mono text-sm whitespace-pre-wrap select-text" class="p-4" />

      <template v-else>
        <!-- A piece is carried by the pointer to another place in the layer, a line marking where it will land; its
             buttons move it one place for a reader without a pointer. -->
        <ol class="flex flex-col" @dragover="over" @drop="dropped">
          <li
            v-for="(piece, index) in pieces"
            :key="piece.id"
            :draggable="!busy"
            data-piece
            class="group flex cursor-grab items-start gap-2 border-b border-default px-4 py-3 active:cursor-grabbing"
            :class="[landing === index ? LINE_BEFORE : '', landing === pieces.length && index === pieces.length - 1 ? LINE_AFTER : '', carrying === index ? 'opacity-50' : '']"
            @dragstart="carry($event, index)"
            @dragend="setDown"
          >
            <span class="w-8 shrink-0 text-sm text-muted tabular-nums">{{ piece.position }}</span>
            <button type="button" class="min-w-0 flex-1 text-start text-sm whitespace-pre-wrap hover:text-primary" @click="emit('open', piece.id)">{{ reader.shown(piece.text) }}</button>
            <span class="shrink-0 text-sm text-muted">{{ typeName(t, te, piece.type) }}</span>
            <div class="flex shrink-0 items-center opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100 pointer-coarse:opacity-100">
              <UTooltip :text="t('brain.action.up')">
                <UButton size="xs" color="neutral" variant="ghost" icon="i-material-symbols-keyboard-arrow-up" :aria-label="t('brain.action.up')" :disabled="index === 0 || busy" @click="swap(index, index - 1)" />
              </UTooltip>
              <UTooltip :text="t('brain.action.down')">
                <UButton size="xs" color="neutral" variant="ghost" icon="i-material-symbols-keyboard-arrow-down" :aria-label="t('brain.action.down')" :disabled="index === pieces.length - 1 || busy" @click="swap(index, index + 1)" />
              </UTooltip>
            </div>
          </li>
        </ol>
        <p v-if="pieces.length === 0" class="p-6 text-center text-sm text-muted">{{ t("brain.status.nothing") }}</p>
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useIsland } from "bmo-ui/app/domain/island";
import { askQuestion } from "bmo-ui/app/domain/question";
import BrainHeading from "./BrainHeading.vue";
import BrainText from "./BrainText.vue";
import { typeName } from "../domain/brain-names";
import * as api from "../domain/brain-api";
import { useReading } from "../domain/brain-reading";
import { carriedTo, PIECE_DRAG } from "../domain/brain-order";

import type { Layer, Memory, MemoryFilter } from "../domain/brain-api";
import type { Carried } from "../domain/brain-order";
import type { Heading } from "../domain/brain-heading";

const props = defineProps<{
  brainId: number;
  heading: Heading;
  layers: Layer[];
  layer: number | "unfiled" | null;
  filter: MemoryFilter;
  changes: number;
}>();

const emit = defineEmits<{ open: [id: number]; changed: [] }>();

const { t, te } = useI18n();
const scroller = ref<HTMLElement | null>(null);
const island = useIsland(t);
const reader = useReading();

const pieces = ref<Memory[]>([]);
const rendering = ref("");
const reading = ref<"pieces" | "rendering">("pieces");
const busy = ref(false);

// The layer shown: the one chosen in the index, the first where none is, and none for the unfiled.
const current = computed(() =>
  props.layer === "unfiled" ? undefined : (props.layers.find((layer) => layer.id === props.layer) ?? props.layers[0])
);
const title = computed(() => {
  const name =
    props.layer === "unfiled"
      ? t("brain.label.unfiled")
      : current.value === undefined
        ? ""
        : props.layer === null
          ? t("brain.label.first_layer", { name: current.value.name })
          : current.value.name;

  return name === "" ? props.heading.title : `${props.heading.title} · ${name}`;
});
// Every filter but the layer narrows the items; the prompt source is the whole layer's and says so while one does.
const narrowed = computed(() => props.heading.chips.some((chip) => chip.choice !== "layer" && chip.honoured));
const notes = computed(() => [
  ...props.heading.notes,
  ...(reading.value === "rendering" && hasPersona.value && narrowed.value ? [t("brain.unhonoured.prompt")] : [])
]);
// Whether the layer holds a live persona item, read whatever the filters narrow the items to.
const hasPersona = ref(false);
const readings = computed(() => [
  { value: "pieces" as const, label: t("brain.label.pieces") },
  { value: "rendering" as const, label: t("brain.label.rendering") }
]);
const exportHref = computed(() => (current.value === undefined ? undefined : api.exportAddress(props.brainId, current.value.name)));

async function read(): Promise<void> {
  const layer = current.value;

  if (layer === undefined && props.layer !== "unfiled") return;

  const filter: MemoryFilter = { ...props.filter, layerId: undefined, unfiled: undefined };

  if (layer === undefined) filter.unfiled = true;
  else filter.layerId = layer.id;

  try {
    pieces.value = (await api.memoryPage(props.brainId, filter, "position", null, 500)).items;

    const persona = layer !== undefined && (await api.memoryPage(props.brainId, { layerId: layer.id, type: "persona", status: "active" }, "position", null, 1)).items.length > 0;

    hasPersona.value = persona;
    rendering.value = persona && layer !== undefined ? await api.persona(props.brainId, layer.name) : "";
  } catch (error) {
    island.fail(error, t("brain.error.read"));
  }

  scroller.value?.scrollTo({ top: 0 });
}

watch(() => [props.brainId, current.value?.id, props.layer, JSON.stringify(props.filter), props.changes], () => void read(), { immediate: true });

watch(() => [pieces.value, reader.language.value] as const, ([shown]) => reader.want(shown.map((piece) => piece.text)), { immediate: true });

// The marks of where a carried piece would land: a line over the piece it lands before, or under the last.
const LINE_BEFORE = "shadow-[inset_0_2px_0_0_var(--ui-primary)]";
const LINE_AFTER = "shadow-[inset_0_-2px_0_0_var(--ui-primary)]";

const carrying = ref<number | null>(null);
// Where among the pieces the carried one would be put down: before the piece at that index.
const landing = ref<number | null>(null);

function carry(event: DragEvent, index: number): void {
  const piece = pieces.value[index];

  if (piece !== undefined) event.dataTransfer?.setData(PIECE_DRAG, JSON.stringify({ id: piece.id, type: piece.type, layerId: piece.layerId ?? null } satisfies Carried));
  if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
  carrying.value = index;
}

function setDown(): void {
  carrying.value = null;
  landing.value = null;
}

/** Where the pointer stands among the pieces: before the first whose middle it has not passed. */
function over(event: DragEvent): void {
  if (carrying.value === null) return;

  event.preventDefault();

  const rows = [...(event.currentTarget as HTMLElement).querySelectorAll<HTMLElement>(":scope > [data-piece]")];
  const at = rows.findIndex((row) => {
    const box = row.getBoundingClientRect();

    return event.clientY < box.top + box.height / 2;
  });

  landing.value = at === -1 ? rows.length : at;
}

/** A piece let go where the line stood: the fewest pieces written again to stand it there. */
async function dropped(event: DragEvent): Promise<void> {
  const from = carrying.value;
  const land = landing.value;

  if (from === null || land === null) return;

  event.preventDefault();
  setDown();
  await write(carriedTo(pieces.value, from, land));
}

// Positions written as new versions, asked first where a persona item is among them.
async function write(positions: { id: number; position: number }[]): Promise<void> {
  if (positions.length === 0) return;

  const persona = positions.some((one) => pieces.value.find((piece) => piece.id === one.id)?.type === "persona");

  if (persona && !(await askQuestion({ title: t("brain.title.persona_reorder"), effect: t("brain.hint.persona_cache"), confirm: t("common.action.save") }))) return;

  busy.value = true;

  try {
    for (const one of positions) await api.writeVersion(props.brainId, one.id, { position: one.position });
    emit("changed");
  } catch (error) {
    island.fail(error, t("brain.error.save"));
  } finally {
    busy.value = false;
  }
}

// Two pieces trade places: each is written again at the other's position, as a new version.
async function swap(from: number, to: number): Promise<void> {
  const a = pieces.value[from];
  const b = pieces.value[to];

  if (a === undefined || b === undefined) return;

  const [aPosition, bPosition] = a.position === b.position ? [b.position + (to > from ? 1 : -1), a.position] : [b.position, a.position];

  await write([
    { id: a.id, position: aPosition },
    { id: b.id, position: bPosition }
  ]);
}
</script>
