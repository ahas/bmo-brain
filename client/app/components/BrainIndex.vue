<template>
  <!-- The index (docs/design.md, 「The screen」): the layers in position order with their live counts and the
       unfiled last, the subjects grouped by type and called by their names, and the types in use, each tab with
       one line saying what it lists. Choosing one narrows the view to it, and choosing it again clears it; the
       chosen row stands highlighted and in sight, and where the view honours no choice the rows say so. A layer's
       description and position are changed here, and a subject's description; a layer is added from the screen's
       create button. An org brain is read-only here: its layers are not carried and nothing is described. A row's
       actions are icon buttons at its end, shown while the row is hovered or focused and always on a coarse
       pointer; a row never grows a second line of buttons. -->
  <div class="flex min-h-0 flex-1 flex-col">
    <!-- The three tabs share the column's width, each its short name whole at the column's default width; a column
         dragged narrower cuts a name, and the cut line's tooltip shows it whole. -->
    <UTabs
      v-model="tab"
      :items="tabs"
      :content="false"
      variant="link"
      class="w-full min-w-0 shrink-0"
      :ui="{ list: 'border-b border-default px-1 min-w-0', trigger: 'flex-1 min-w-0 justify-center px-1' }"
    />
    <div class="border-b border-default px-3 py-2 text-sm text-muted">
      <p>{{ t(`brain.index.${tab}_about`) }} {{ t("brain.index.again_clears") }}</p>
      <p v-if="!usable" class="mt-1 text-warning">{{ t("brain.unhonoured.timeline") }}</p>
    </div>
    <div class="border-b border-default p-2">
      <UInput v-model="words" icon="i-material-symbols-search" :placeholder="t('brain.label.index_search')" size="sm" class="w-full" />
    </div>

    <div ref="scroller" class="min-h-0 flex-1 overflow-y-auto" :class="{ 'pointer-events-none opacity-50': !usable }" :aria-disabled="!usable">
      <!-- A layer is carried by the pointer to another place among the layers, a line marking where it will land; a
           piece of the layer view dropped on a layer, or on the unfiled, moves there as a new version. -->
      <ul v-if="tab === 'layers'" ref="layerList" class="flex flex-col py-1" @dragover="layerOver" @dragleave="layerLeft" @drop="layerDropped">
        <li
          v-for="(layer, index) in shownLayers"
          :key="layer.id"
          :draggable="usable && !readOnly"
          data-layer
          class="group flex items-stretch hover:bg-elevated"
          :class="[
            readOnly ? '' : 'cursor-grab active:cursor-grabbing',
            { 'bg-primary/10 shadow-[inset_3px_0_0_var(--ui-primary)]': place.layer === layer.id },
            landing === index ? LINE_BEFORE : '',
            landing === shownLayers.length && index === shownLayers.length - 1 ? LINE_AFTER : '',
            pieceOver === layer.id ? 'bg-primary/10 ring-1 ring-primary ring-inset' : ''
          ]"
          :data-chosen="place.layer === layer.id || undefined"
          @dragstart="carryLayer($event, index)"
          @dragend="setLayerDown"
          @dragenter="pieceEnter($event, layer.id)"
          @dragleave="pieceLeave($event, layer.id)"
        >
          <button
            type="button"
            class="flex min-w-0 flex-1 items-start gap-2 py-3 ps-3 text-start"
            :aria-pressed="place.layer === layer.id"
            :disabled="!usable"
            @click="emit('choose', { layer: place.layer === layer.id ? null : layer.id })"
          >
            <span v-if="readOnly" class="size-5 shrink-0" aria-hidden="true" />
            <UIcon v-else name="i-material-symbols-drag-indicator" class="size-5 shrink-0 text-muted" aria-hidden="true" />
            <div class="min-w-0 flex-1">
              <div class="truncate text-sm font-medium">{{ layer.name }}</div>
              <div v-if="layer.description" class="truncate text-sm text-muted">{{ layer.description }}</div>
            </div>
            <UBadge color="neutral" variant="soft" size="sm"><RollingNumber :value="layerCount(String(layer.id))" /></UBadge>
          </button>
          <div v-if="!readOnly" class="flex items-center pe-2 ps-1 opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100 pointer-coarse:opacity-100">
            <UTooltip :text="t('brain.action.describe')">
              <UButton size="xs" color="neutral" variant="ghost" icon="i-material-symbols-edit-outline" :aria-label="t('brain.action.describe')" @click="describeLayer(layer)" />
            </UTooltip>
          </div>
        </li>
        <li data-unfiled @dragenter="pieceEnter($event, 'unfiled')" @dragleave="pieceLeave($event, 'unfiled')">
          <button
            type="button"
            class="flex w-full items-center gap-2 px-3 py-3 text-start hover:bg-elevated"
            :class="[{ 'bg-primary/10 shadow-[inset_3px_0_0_var(--ui-primary)]': place.layer === 'unfiled' }, pieceOver === 'unfiled' ? 'bg-primary/10 ring-1 ring-primary ring-inset' : '']"
            :data-chosen="place.layer === 'unfiled' || undefined"
            :aria-pressed="place.layer === 'unfiled'"
            :disabled="!usable"
            @click="emit('choose', { layer: place.layer === 'unfiled' ? null : 'unfiled' })"
          >
            <!-- No handle, since the unfiled is not carried; its name keeps the layers' column. -->
            <span class="size-5 shrink-0" aria-hidden="true" />
            <span class="min-w-0 flex-1 truncate text-sm text-muted">{{ t("brain.label.unfiled") }}</span>
            <UBadge color="neutral" variant="soft" size="sm"><RollingNumber :value="layerCount(null)" /></UBadge>
          </button>
        </li>
      </ul>

      <div v-else-if="tab === 'subjects'" class="flex flex-col py-1">
        <section v-for="[kind, members] in subjectGroups" :key="kind">
          <h3 class="px-3 pt-3 pb-1 text-sm font-semibold text-muted">{{ subjectTypeName(t, te, kind) }}</h3>
          <ul>
            <li v-for="subject in members" :key="subject.id" class="group flex items-stretch hover:bg-elevated" :class="{ 'bg-primary/10 shadow-[inset_3px_0_0_var(--ui-primary)]': place.subject === subject.id }" :data-chosen="place.subject === subject.id || undefined">
              <button
                type="button"
                class="flex min-w-0 flex-1 items-start gap-2 py-2 ps-3 text-start"
                :aria-pressed="place.subject === subject.id"
                :disabled="!usable"
                @click="emit('choose', { subject: place.subject === subject.id ? null : subject.id })"
              >
                <div class="min-w-0 flex-1">
                  <div class="truncate text-sm font-medium">{{ subject.name ?? subject.key }}</div>
                  <div class="truncate text-sm text-muted">{{ subject.key }}<template v-if="subject.description"> · {{ subject.description }}</template></div>
                </div>
                <UBadge color="neutral" variant="soft" size="sm"><RollingNumber :value="subjectCount(subject.id)" /></UBadge>
              </button>
              <div class="flex items-center gap-0.5 pe-2 ps-1 opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100 pointer-coarse:opacity-100">
                <UTooltip v-if="!readOnly" :text="t('brain.action.describe')">
                  <UButton size="xs" color="neutral" variant="ghost" icon="i-material-symbols-edit-outline" :aria-label="t('brain.action.describe')" @click="describeSubject(subject)" />
                </UTooltip>
                <UTooltip :text="t('brain.view.map')">
                  <UButton size="xs" color="neutral" variant="ghost" icon="i-material-symbols-hub-outline" :aria-label="t('brain.view.map')" @click="emit('choose', { subject: subject.id, view: 'map' })" />
                </UTooltip>
              </div>
            </li>
          </ul>
        </section>
        <p v-if="subjectGroups.length === 0" class="p-4 text-sm text-muted">{{ t("brain.status.nothing") }}</p>
      </div>

      <ul v-else class="flex flex-col py-1">
        <li v-for="type in shownTypes" :key="type.type">
          <button
            type="button"
            class="flex w-full items-center gap-2 px-3 py-2 text-start hover:bg-elevated"
            :class="{ 'bg-primary/10 shadow-[inset_3px_0_0_var(--ui-primary)]': place.type === type.type }"
            :data-chosen="place.type === type.type || undefined"
            :aria-pressed="place.type === type.type"
            :disabled="!usable"
            @click="emit('choose', { type: place.type === type.type ? EVERY_TYPE_BUT_EPISODES : type.type })"
          >
            <span class="min-w-0 flex-1 truncate text-sm">{{ typeName(t, te, type.type) }}</span>
            <UBadge v-if="type.reserved" color="primary" variant="soft" size="sm" :label="t('brain.label.reserved')" />
            <UBadge color="neutral" variant="soft" size="sm"><RollingNumber :value="type.count" /></UBadge>
          </button>
        </li>
      </ul>
    </div>

    <UModal v-model:open="editing" :title="t('brain.action.describe')">
      <template #body>
        <div class="flex flex-col gap-3">
          <UFormField :label="t('brain.field.description')">
            <UTextarea v-model="editDescription" :rows="3" autoresize class="w-full" />
          </UFormField>
          <UFormField v-if="editKind === 'layer'" :label="t('brain.field.position')">
            <UInput v-model.number="editPosition" type="number" class="w-32" />
          </UFormField>
        </div>
      </template>
      <template #footer>
        <div class="flex w-full justify-end gap-2">
          <UButton color="neutral" variant="ghost" :label="t('common.action.cancel')" @click="editing = false" />
          <UButton :label="t('common.action.save')" :loading="saving" @click="save" />
        </div>
      </template>
    </UModal>
  </div>
</template>

<script setup lang="ts">
import { useIsland } from "bmo-ui/app/domain/island";
import * as api from "../domain/brain-api";
import { subjectTypeName, typeName } from "../domain/brain-names";
import { EVERY_TYPE_BUT_EPISODES, honours } from "../domain/brain-place";
import { askQuestion } from "bmo-ui/app/domain/question";
import { carriedTo, PIECE_DRAG } from "../domain/brain-order";

import type { Layer, Subject, Tally, TypeCount } from "../domain/brain-api";
import type { Carried } from "../domain/brain-order";
import type { Place } from "../domain/brain-place";

const props = defineProps<{
  brainId: number | null;
  layers: Layer[];
  layerCounts: Tally[];
  subjects: Subject[];
  subjectCounts: Tally[];
  types: TypeCount[];
  place: Place;
  // Whether the brain chosen is an org brain, which the screen reads and never writes (docs/design.md, 「The screen」).
  readOnly: boolean;
}>();

const emit = defineEmits<{ choose: [patch: Partial<Place>]; changed: [] }>();

const { t, te } = useI18n();
const island = useIsland(t);

type Tab = "layers" | "subjects" | "types";

const tab = ref<Tab>("layers");
const words = ref("");
const scroller = ref<HTMLElement | null>(null);
const layerList = ref<HTMLElement | null>(null);

// --- A layer carried among the layers, and a piece of the layer view carried onto one ---

// The marks of where a carried layer would land: a line over the layer it lands before, or under the last.
const LINE_BEFORE = "shadow-[inset_0_2px_0_0_var(--ui-primary)]";
const LINE_AFTER = "shadow-[inset_0_-2px_0_0_var(--ui-primary)]";

const carrying = ref<number | null>(null);
const landing = ref<number | null>(null);
// The layer, or the unfiled, a carried piece stands over.
const pieceOver = ref<number | "unfiled" | null>(null);

function carryLayer(event: DragEvent, index: number): void {
  if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
  event.dataTransfer?.setData("text/plain", shownLayers.value[index]?.name ?? "");
  carrying.value = index;
}

function setLayerDown(): void {
  carrying.value = null;
  landing.value = null;
}

function carriesPiece(event: DragEvent): boolean {
  return event.dataTransfer?.types.includes(PIECE_DRAG) ?? false;
}

function pieceEnter(event: DragEvent, layer: number | "unfiled"): void {
  if (carriesPiece(event) && !props.readOnly) pieceOver.value = layer;
}

function pieceLeave(event: DragEvent, layer: number | "unfiled"): void {
  // A pointer passing onto a child of the row has not left it.
  if ((event.currentTarget as HTMLElement).contains(event.relatedTarget as Node | null)) return;
  if (pieceOver.value === layer) pieceOver.value = null;
}

/** A layer carried: where among the layers it would be put down. A piece carried: the row under it takes it. */
function layerOver(event: DragEvent): void {
  if (props.readOnly) return;

  if (carriesPiece(event)) {
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = "move";

    return;
  }

  if (carrying.value === null || layerList.value === null) return;

  event.preventDefault();

  const rows = [...layerList.value.querySelectorAll<HTMLElement>(":scope > [data-layer]")];
  const at = rows.findIndex((row) => {
    const box = row.getBoundingClientRect();

    return event.clientY < box.top + box.height / 2;
  });

  landing.value = at === -1 ? rows.length : at;
}

/** The pointer gone out of the layers: no line stands where a carried layer will not land. */
function layerLeft(event: DragEvent): void {
  if (!(event.currentTarget as HTMLElement).contains(event.relatedTarget as Node | null)) landing.value = null;
}

async function layerDropped(event: DragEvent): Promise<void> {
  const brainId = props.brainId;

  if (brainId === null || props.readOnly) return;

  if (carriesPiece(event)) {
    event.preventDefault();

    const target = pieceOver.value;

    pieceOver.value = null;
    if (target === null) return;

    await movePiece(brainId, JSON.parse(event.dataTransfer!.getData(PIECE_DRAG)) as Carried, target);

    return;
  }

  const from = carrying.value;
  const land = landing.value;

  if (from === null || land === null) return;

  event.preventDefault();
  setLayerDown();

  try {
    for (const one of carriedTo(shownLayers.value, from, land)) await api.changeLayer(brainId, one.id, { position: one.position });
    emit("changed");
  } catch (error) {
    island.fail(error, t("brain.error.save"));
  }
}

// A piece moved to a layer, or to none, as a new version; a persona piece is asked first.
async function movePiece(brainId: number, piece: Carried, target: number | "unfiled"): Promise<void> {
  if (piece.layerId === (target === "unfiled" ? null : target)) return;
  if (piece.type === "persona" && !(await askQuestion({ title: t("brain.title.persona_save"), effect: t("brain.hint.persona_cache"), confirm: t("common.action.save") }))) return;

  try {
    await api.writeVersion(brainId, piece.id, { layerId: target === "unfiled" ? null : target });
    emit("changed");
  } catch (error) {
    island.fail(error, t("brain.error.save"));
  }
}

// Whether the view open honours the index's choices; the version timeline honours none.
const usable = computed(() => honours(props.place.view, "layer"));

// The tab holding the newest choice is the one open, and the chosen row is scrolled into sight.
watch(
  () => [props.place.layer, props.place.subject, props.place.type] as const,
  ([layer, subject, type], before) => {
    const typed = type !== EVERY_TYPE_BUT_EPISODES && !type.startsWith("*");

    if (subject !== null && subject !== before?.[1]) tab.value = "subjects";
    else if (layer !== null && layer !== before?.[0]) tab.value = "layers";
    else if (typed && type !== before?.[2]) tab.value = "types";

    void nextTick(() => scroller.value?.querySelector("[data-chosen]")?.scrollIntoView({ block: "nearest" }));
  },
  { immediate: true }
);

watch(tab, () => void nextTick(() => scroller.value?.querySelector("[data-chosen]")?.scrollIntoView({ block: "nearest" })));

const tabs = computed(() => [
  { value: "layers", label: t("brain.index.layers") },
  { value: "subjects", label: t("brain.index.subjects") },
  { value: "types", label: t("brain.index.types") }
]);

function holds(texts: (string | null | undefined)[]): boolean {
  const asked = words.value.trim().toLocaleLowerCase();

  return asked === "" || texts.some((text) => (text ?? "").toLocaleLowerCase().includes(asked));
}

const shownLayers = computed(() => props.layers.filter((layer) => holds([layer.name, layer.description])));
const shownTypes = computed(() => props.types.filter((type) => holds([type.type])));

const subjectGroups = computed(() => {
  const groups = new Map<string, Subject[]>();

  for (const subject of props.subjects) {
    if (!holds([subject.type, subject.key, subject.description, subject.name])) continue;

    groups.set(subject.type, [...(groups.get(subject.type) ?? []), subject]);
  }

  return [...groups.entries()].sort(([a], [b]) => a.localeCompare(b));
});

function layerCount(group: string | null): number {
  return props.layerCounts.find((tally) => tally.group === group)?.count ?? 0;
}

function subjectCount(id: number): number {
  return props.subjectCounts.find((tally) => tally.group === String(id))?.count ?? 0;
}

const editing = ref(false);
const saving = ref(false);
const editKind = ref<"layer" | "subject">("layer");
const editId = ref(0);
const editDescription = ref("");
const editPosition = ref(0);

function describeLayer(layer: Layer): void {
  editKind.value = "layer";
  editId.value = layer.id;
  editDescription.value = layer.description;
  editPosition.value = layer.position;
  editing.value = true;
}

function describeSubject(subject: Subject): void {
  editKind.value = "subject";
  editId.value = subject.id;
  editDescription.value = subject.description;
  editing.value = true;
}

async function save(): Promise<void> {
  if (props.brainId === null) return;

  saving.value = true;

  try {
    if (editKind.value === "layer") await api.changeLayer(props.brainId, editId.value, { description: editDescription.value, position: editPosition.value });
    else await api.changeSubject(props.brainId, editId.value, editDescription.value);

    editing.value = false;
    emit("changed");
  } catch (error) {
    island.fail(error, t("brain.error.save"));
  } finally {
    saving.value = false;
  }
}
</script>
