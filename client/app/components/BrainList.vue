<template>
  <!-- The memories under the toolbar's filters (docs/design.md, 「The screen」): a table where there is room
       and a list of items on a phone, each row keyed by its id so rows the brain writes or moves come and
       go where they stand. A row opens the memory. -->
  <div class="flex min-h-0 flex-1 flex-col">
    <ContentSkeleton v-if="memories === null" class="p-3" :rows="10" list-class="flex flex-col gap-2" :label="t('brain.status.loading')">
      <SkeletonBar class="h-8 w-full" />
    </ContentSkeleton>

    <div v-else class="min-h-0 flex-1 overflow-y-auto">
      <BrainSheet v-if="subjectId !== null" :brain-id="brainId" :subject-id="subjectId" :changes="changes" />
      <p v-if="failure" class="p-4 text-sm text-error">{{ failure }}</p>
      <p v-else-if="memories.length === 0" class="p-6 text-center text-sm text-muted">{{ t("brain.status.nothing") }}</p>

      <div v-if="!narrow && memories.length > 0" role="row" class="sticky top-0 z-10 grid grid-cols-[5.5rem_minmax(0,1fr)_6rem_5.5rem_2.5rem_6rem] gap-3 border-b border-default bg-default px-3 py-2 text-xs font-medium text-muted">
        <span>{{ t("brain.field.type") }}</span>
        <span>{{ t("brain.field.text") }}</span>
        <span>{{ t("brain.field.layer") }}</span>
        <span>{{ t("brain.field.status") }}</span>
        <span class="text-end">{{ t("brain.field.importance_short") }}</span>
        <span class="text-end">{{ t("brain.field.valid_from") }}</span>
      </div>

      <MovingList tag="ul" class="flex flex-col">
        <li v-for="memory in memories" :key="memory.id">
          <button
            type="button"
            class="w-full border-b border-default px-3 py-2 text-start hover:bg-elevated"
            :class="{ 'bg-elevated': memory.id === openId }"
            @click="emit('open', memory.id)"
          >
            <div v-if="!narrow" class="grid grid-cols-[5.5rem_minmax(0,1fr)_6rem_5.5rem_2.5rem_6rem] items-center gap-3 text-sm">
              <span class="truncate text-muted">{{ memory.type }}</span>
              <span class="truncate" :title="reader.of(memory.text) === undefined ? undefined : memory.text">{{ reader.shown(memory.text) }}</span>
              <span class="truncate text-muted">{{ layerName(memory.layerId) }}</span>
              <BrainStatus :status="memory.status" />
              <span class="text-end tabular-nums">{{ memory.importance }}</span>
              <span class="text-end text-xs text-muted tabular-nums">{{ day(memory.validFrom) }}</span>
            </div>
            <div v-else class="flex flex-col gap-1">
              <div class="flex items-center gap-2 text-xs text-muted">
                <span>{{ memory.type }}</span>
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
import BrainSheet from "./BrainSheet.vue";
import BrainStatus from "./BrainStatus.vue";
import { useReading } from "../domain/brain-reading";

import type { Layer, Memory } from "../domain/brain-api";

const props = defineProps<{
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

const { t } = useI18n();
const reader = useReading();

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
