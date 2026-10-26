<template>
  <!-- The memories under the toolbar's filters (docs/design.md, 「The screen」): a table where there is room
       and a list of items on a phone, each row keyed by its id so rows the brain writes or moves come and
       go where they stand. A row opens the memory. -->
  <div class="flex min-h-0 flex-1 flex-col">
    <BrainHeading v-bind="heading" :count="memories === null ? null : count" />
    <ContentSkeleton v-if="memories === null" class="p-3" :rows="10" list-class="flex flex-col gap-2" :label="t('brain.status.loading')">
      <SkeletonBar class="h-8 w-full" />
    </ContentSkeleton>

    <div v-else ref="scroller" class="min-h-0 flex-1 overflow-y-auto">
      <BrainSheet v-if="subjectId !== null" :brain-id="brainId" :subject-id="subjectId" :name="subjectNames.get(subjectId) ?? ''" :changes="changes" />
      <p v-if="failure" class="p-4 text-sm text-error">{{ failure }}</p>
      <p v-else-if="memories.length === 0" class="p-6 text-center text-sm text-muted">{{ t("brain.status.nothing") }}</p>

      <div v-if="!stacked && memories.length > 0" role="row" class="sticky top-0 z-10 grid gap-3 border-b border-default bg-default px-3 py-2 text-xs font-medium text-muted" :class="COLUMNS">
        <span class="truncate">{{ t("brain.field.type") }}</span>
        <span class="truncate">{{ t("brain.field.text") }}</span>
        <span class="truncate">{{ t("brain.field.layer") }}</span>
        <span class="truncate">{{ t("brain.field.status") }}</span>
        <span class="truncate text-end">{{ t("brain.field.importance_short") }}</span>
        <span class="truncate text-end">{{ t("brain.field.valid_from") }}</span>
      </div>

      <MovingList tag="ul" class="flex flex-col">
        <li v-for="memory in memories" :key="memory.id">
          <button
            type="button"
            class="w-full border-b border-default px-3 py-2 text-start hover:bg-elevated"
            :class="{ 'bg-elevated': memory.id === openId }"
            @click="emit('open', memory.id)"
          >
            <div v-if="!stacked" class="grid items-center gap-3 text-sm" :class="COLUMNS">
              <span class="truncate text-muted">{{ typeName(t, te, memory.type) }}</span>
              <span class="truncate" :title="reader.of(memory.text) === undefined ? undefined : memory.text">{{ reader.shown(memory.text) }}</span>
              <span class="truncate text-muted">{{ layerName(memory.layerId) }}</span>
              <BrainStatus :status="memory.status" />
              <span class="text-end tabular-nums">{{ memory.importance }}</span>
              <span class="text-end text-xs text-muted tabular-nums">{{ day(memory.validFrom) }}</span>
            </div>
            <div v-else class="flex flex-col gap-1">
              <div class="flex items-center gap-2 text-xs text-muted">
                <span>{{ typeName(t, te, memory.type) }}</span>
                <BrainStatus :status="memory.status" />
                <span class="ms-auto tabular-nums">{{ day(memory.validFrom) }}</span>
              </div>
              <p class="line-clamp-3 text-sm">{{ reader.shown(memory.text) }}</p>
              <div v-if="memory.subjects.length > 0" class="truncate text-xs text-muted">
                {{ memory.subjects.map((subject) => subjectNames.get(subject.id) ?? subject.key).join(" · ") }}
              </div>
            </div>
          </button>
        </li>
      </MovingList>

      <div v-if="more" class="flex justify-center p-3">
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

import type { Layer, Memory } from "../domain/brain-api";
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
const reader = useReading();
const scroller = ref<HTMLElement | null>(null);

// The table's columns (docs/design.md, 「The screen」): the text takes what is left, at least 8rem, and every other
// column one narrow width of its own, as wide as its header and its usual values. Where the pane is narrower than the
// table's least width — the columns, their gaps and the row's side room — the rows stand stacked instead.
const COLUMNS = "grid-cols-[5rem_minmax(8rem,1fr)_6rem_4.5rem_2.5rem_5.5rem]";
const TABLE_LEAST_PX = (5 + 8 + 6 + 4.5 + 2.5 + 5.5) * 16 + 5 * 12 + 2 * 12;
const paneWidth = ref<number | null>(null);
const stacked = computed(() => props.narrow || (paneWidth.value !== null && paneWidth.value < TABLE_LEAST_PX));
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

// A changed choice shows its memories from their start.
watch(
  () => JSON.stringify(props.heading.chips.map((chip) => [chip.choice, chip.value])),
  () => scroller.value?.scrollTo({ top: 0 })
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
