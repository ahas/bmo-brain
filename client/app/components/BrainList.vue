<template>
  <!-- The memories under the toolbar's filters (docs/design.md, 「The screen」): the console's shared table where its
       pane holds it, and a list of items, each keyed by its id so items the brain writes or moves come and go where
       they stand, where it does not. A row or an item opens the memory. -->
  <div class="flex min-h-0 flex-1 flex-col">
    <BrainHeading v-bind="heading" :count="memories === null ? null : count" />
    <ContentSkeleton v-if="memories === null" class="p-3" :rows="10" list-class="flex flex-col gap-2" :label="t('brain.status.loading')">
      <SkeletonBar class="h-8 w-full" />
    </ContentSkeleton>

    <!-- The pane is measured for the table: where it holds it the memories stand in the shared table, which fills
         the pane and scrolls its own rows, or under a subject's sheet takes its own height while the pane scrolls
         both; where it does not, they stand stacked. -->
    <div v-else ref="scroller" class="flex min-h-0 min-w-0 flex-1 flex-col" :class="tableFills ? '' : 'overflow-y-auto'">
      <BrainSheet v-if="subjectId !== null" :brain-id="brainId" :subject-id="subjectId" :name="subjectNames.get(subjectId) ?? ''" :changes="changes" />
      <p v-if="failure" class="p-4 text-sm text-error">{{ failure }}</p>
      <p v-else-if="memories.length === 0" class="p-6 text-center text-sm text-muted">{{ t("brain.status.nothing") }}</p>

      <DataTable
        v-if="!stacked && memories.length > 0"
        data-memory-table
        :data="memories"
        :columns="columns"
        layout="dense"
        selectable
        :class="tableFills ? 'min-h-0 flex-1' : 'shrink-0'"
        :row-class="(memory: Memory) => (memory.id === openId ? 'bg-elevated' : '')"
        @select="(memory: Memory) => emit('open', memory.id)"
      >
        <template #type-cell="{ row }">{{ typeName(t, te, row.original.type) }}</template>
        <template #text-cell="{ row }">
          <span :title="reader.of(row.original.text) === undefined ? undefined : row.original.text">{{ reader.shown(row.original.text) }}</span>
        </template>
        <template #layer-cell="{ row }">{{ layerName(row.original.layerId) }}</template>
        <template #status-cell="{ row }"><BrainStatus :status="row.original.status" /></template>
        <template #importance-cell="{ row }">{{ row.original.importance }}</template>
        <template #validFrom-cell="{ row }">{{ day(row.original.validFrom) }}</template>
      </DataTable>

      <MovingList v-else-if="stacked" tag="ul" class="flex flex-col">
        <li v-for="memory in memories" :key="memory.id">
          <button
            type="button"
            class="w-full border-b border-default px-3 py-2 text-start hover:bg-elevated"
            :class="{ 'bg-elevated': memory.id === openId }"
            @click="emit('open', memory.id)"
          >
            <div class="flex flex-col gap-1">
              <div class="flex items-center gap-2 text-sm text-muted">
                <span>{{ typeName(t, te, memory.type) }}</span>
                <BrainStatus :status="memory.status" />
                <span class="ms-auto tabular-nums">{{ day(memory.validFrom) }}</span>
              </div>
              <p class="line-clamp-3 text-sm">{{ reader.shown(memory.text) }}</p>
              <div v-if="memory.subjects.length > 0" class="truncate text-sm text-muted">
                {{ memory.subjects.map((subject) => subjectNames.get(subject.id) ?? subject.key).join(" · ") }}
              </div>
            </div>
          </button>
        </li>
      </MovingList>

      <div v-if="more" class="flex shrink-0 justify-center p-3">
        <UButton color="neutral" variant="outline" :loading="loadingMore" :label="t('brain.action.more')" @click="emit('more')" />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import BrainHeading from "./BrainHeading.vue";
import BrainSheet from "./BrainSheet.vue";
import { typeName } from "../domain/brain-names";
import BrainStatus from "./BrainStatus.vue";
import { useReading } from "../domain/brain-reading";
import { MEMORY_COLUMNS, STACK_BELOW_PX } from "../domain/brain-table";

import type { TableColumn } from "@nuxt/ui";
import type { Layer, Memory } from "../domain/brain-api";
import type { MemoryColumnId } from "../domain/brain-table";
import type { Heading } from "../domain/brain-heading";

const props = defineProps<{
  heading: Heading;
  count: number;
  memories: Memory[] | null;
  more: boolean;
  loadingMore: boolean;
  failure: string;
  layers: Layer[];
  subjectNames: Map<number, string>;
  openId: number | null;
  narrow: boolean;
  brainId: number;
  subjectId: number | null;
  changes: number;
}>();

const emit = defineEmits<{ open: [id: number]; more: [] }>();

const { t, te } = useI18n();

// Each column's name, in the bundle's words.
const HEADERS: Readonly<Record<MemoryColumnId, string>> = {
  type: "brain.field.type",
  text: "brain.field.text",
  layer: "brain.field.layer",
  status: "brain.field.status",
  importance: "brain.field.importance_short",
  validFrom: "brain.field.valid_from"
};
const reader = useReading();
const scroller = ref<HTMLElement | null>(null);

// The table's columns, each named and laid out by its own rule (docs/design.md, 「The screen」; `brain-table.ts`), and
// the pane's width, under which the rows stand stacked instead.
const columns = computed<TableColumn<Memory>[]>(() =>
  MEMORY_COLUMNS.map((column) => ({ id: column.id, header: t(HEADERS[column.id]), meta: { class: { th: column.th, td: column.td } } }))
);
const paneWidth = ref<number | null>(null);
const stacked = computed(() => props.narrow || (paneWidth.value !== null && paneWidth.value < STACK_BELOW_PX));
// The table fills the pane and scrolls its own rows, but for under a subject's sheet, where the pane scrolls both.
const tableFills = computed(() => !stacked.value && props.subjectId === null);
let sized: ResizeObserver | null = null;

watch(
  scroller,
  (element) => {
    sized?.disconnect();
    sized = null;

    if (element === null) return;

    sized = new ResizeObserver(([entry]) => {
      paneWidth.value = entry?.contentRect.width ?? null;
    });
    sized.observe(element);
  },
  { flush: "post" }
);

onBeforeUnmount(() => sized?.disconnect());

// A changed choice shows its memories from their start, whichever box scrolls them.
watch(
  () => JSON.stringify(props.heading.chips.map((chip) => [chip.choice, chip.value])),
  () => {
    scroller.value?.scrollTo({ top: 0 });
    scroller.value?.querySelector("[data-memory-table]")?.scrollTo({ top: 0 });
  }
);

// Each page's texts are asked for in the reading language as the page comes; the English stands meanwhile.
watch(
  () => [props.memories, reader.language.value] as const,
  ([memories]) => reader.want((memories ?? []).map((memory) => memory.text)),
  { immediate: true }
);

function layerName(id: number | null | undefined): string {
  if (id === null || id === undefined) return "";

  return props.layers.find((layer) => layer.id === id)?.name ?? `#${id}`;
}

function day(at: number): string {
  return new Date(at).toLocaleDateString();
}
</script>
