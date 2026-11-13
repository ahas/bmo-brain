<template>
  <!-- The version timeline (docs/design.md, 「The screen」): one lineage on a time axis, each version a point
       at its validFrom and a bar for the span it held, its colour its status and its mark its writer. Rows a
       merge folded into a version reach it as lines from above; the episodes a version rests on stand under
       the axis. A point opens the version below with the words it changed from the one before. -->
  <div class="flex min-h-0 flex-1 flex-col">
    <BrainHeading v-bind="heading" :title="memoryId === null ? heading.title : `${heading.title} · #${memoryId}`" :count="versions === null || memoryId === null ? null : versions.length" />
    <div class="flex min-h-0 flex-1 flex-col overflow-y-auto pb-(--bmo-create-room)">
    <p v-if="memoryId === null" class="p-6 text-center text-sm text-muted">{{ t("brain.status.timeline_empty") }}</p>
    <ContentSkeleton v-else-if="versions === null" class="p-4" :rows="4" list-class="flex flex-col gap-3" :label="t('brain.status.loading')">
      <SkeletonBar class="h-6 w-full" />
    </ContentSkeleton>
    <template v-else>
      <div ref="frame" class="w-full shrink-0 px-2 pt-2">
        <svg :viewBox="`0 0 ${WIDTH} ${HEIGHT}`" class="h-auto w-full" role="img" :aria-label="t('brain.view.timeline')">
          <line :x1="PAD" :x2="WIDTH - PAD" :y1="AXIS" :y2="AXIS" class="stroke-(--ui-border-accented)" stroke-width="1" />
          <text :x="PAD" :y="HEIGHT - 6" class="fill-(--ui-text-muted) text-[10px]">{{ day(span.from) }}</text>
          <text :x="WIDTH - PAD" :y="HEIGHT - 6" text-anchor="end" class="fill-(--ui-text-muted) text-[10px]">{{ day(span.to) }}</text>

          <g v-for="row in merged" :key="`m${row.id}`">
            <line :x1="x(row.validFrom)" :y1="MERGED" :x2="x(row.validTo ?? row.validFrom)" :y2="AXIS" class="stroke-(--ui-text-dimmed)" stroke-dasharray="3 3" />
            <circle :cx="x(row.validFrom)" :cy="MERGED" r="4" class="cursor-pointer fill-(--ui-bg-accented) stroke-(--ui-text-dimmed)" @click="emit('open', row.id)">
              <title>#{{ row.id }} {{ row.text }}</title>
            </circle>
          </g>

          <g v-for="version in versions" :key="version.id">
            <rect :x="x(version.validFrom)" :y="AXIS - 3" :width="Math.max(1, x(version.validTo ?? now) - x(version.validFrom))" height="6" :fill="colour(version.status)" opacity="0.35" />
            <g class="cursor-pointer" @click="chosen = version.id">
              <circle v-if="markOf(version) === 'person'" :cx="x(version.validFrom)" :cy="AXIS" r="7" :fill="colour(version.status)" :stroke-width="chosen === version.id ? 3 : 0" class="stroke-(--ui-text-highlighted)" />
              <rect v-else-if="markOf(version) === 'tool'" :x="x(version.validFrom) - 6" :y="AXIS - 6" width="12" height="12" :fill="colour(version.status)" :stroke-width="chosen === version.id ? 3 : 0" class="stroke-(--ui-text-highlighted)" />
              <path v-else :d="`M ${x(version.validFrom)} ${AXIS - 8} l 7 8 l -7 8 l -7 -8 z`" :fill="colour(version.status)" :stroke-width="chosen === version.id ? 3 : 0" class="stroke-(--ui-text-highlighted)" />
              <title>#{{ version.id }} · {{ t(`brain.status_name.${version.status}`) }} · {{ version.actorName }}</title>
            </g>
            <text v-if="version.validTo !== null && version.validTo !== undefined && (version.status === 'done' || version.status === 'invalidated')" :x="x(version.validTo)" :y="AXIS - 12" text-anchor="middle" class="fill-(--ui-text-muted) text-[12px]">⊣</text>
          </g>

          <g v-for="ground in groundRows" :key="`g${ground.id}`">
            <line :x1="x(ground.validFrom)" :x2="x(ground.validFrom)" :y1="AXIS + 4" :y2="GROUND - 4" class="stroke-(--ui-border-accented)" />
            <circle :cx="x(ground.validFrom)" :cy="GROUND" r="4" class="cursor-pointer fill-(--ui-text-dimmed)" @click="emit('open', ground.id)">
              <title>#{{ ground.id }} {{ ground.text }}</title>
            </circle>
          </g>
        </svg>
        <div class="flex flex-wrap gap-3 px-2 pb-2 text-sm text-muted">
          <span>◆ {{ t("brain.label.writer_agent") }}</span>
          <span>■ {{ t("brain.label.writer_tool") }}</span>
          <span>● {{ t("brain.label.writer_person") }}</span>
          <span v-if="merged.length > 0">┄ {{ t("brain.label.merged_in") }}</span>
          <span v-if="groundRows.length > 0">• {{ t("brain.field.grounds") }}</span>
        </div>
      </div>

      <div v-if="chosenVersion" class="flex flex-col gap-2 border-t border-default p-4">
        <div class="flex items-center gap-2 text-sm text-muted">
          <BrainStatus :status="chosenVersion.status" />
          <span class="tabular-nums">#{{ chosenVersion.id }} · {{ stamp(chosenVersion.validFrom) }}</span>
          <span>· {{ chosenVersion.actorName }}</span>
          <UButton size="xs" color="neutral" variant="ghost" class="ms-auto" :label="t('brain.action.open')" @click="emit('open', chosenVersion.id)" />
        </div>
        <BrainText :text="chosenVersion.text" text-class="text-sm whitespace-pre-wrap select-text">
          <p class="text-sm whitespace-pre-wrap select-text">
            <span v-for="(piece, index) in change" :key="index" :class="{ 'bg-success/20': piece.change === 'added', 'bg-error/20 line-through': piece.change === 'removed' }">{{ piece.text }}</span>
          </p>
        </BrainText>
      </div>
    </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useIsland } from "bmo-ui/app/domain/island";
import BrainHeading from "./BrainHeading.vue";
import BrainStatus from "./BrainStatus.vue";
import BrainText from "./BrainText.vue";
import * as api from "../domain/brain-api";
import { wordChange } from "../domain/brain-words";

import type { Memory, MemoryStatus } from "../domain/brain-api";
import type { Heading } from "../domain/brain-heading";

const props = defineProps<{ brainId: number; heading: Heading; memoryId: number | null; changes: number }>();

const emit = defineEmits<{ open: [id: number] }>();

const { t } = useI18n();
const island = useIsland(t);

const WIDTH = 720;
const HEIGHT = 150;
const PAD = 24;
const MERGED = 22;
const AXIS = 70;
const GROUND = 118;

const versions = ref<Memory[] | null>(null);
const merged = ref<Memory[]>([]);
const groundRows = ref<Memory[]>([]);
const chosen = ref<number | null>(null);
const now = Date.now();

async function read(): Promise<void> {
  if (props.memoryId === null) return;

  try {
    const found = await api.memory(props.brainId, props.memoryId);
    const lineage = await api.lineage(props.brainId, found.lineageId);
    const ids = lineage.map((version) => version.id);
    const episodeIds = [
      ...new Set(lineage.flatMap((version) => version.subjects.filter((subject) => subject.type === "episode").map((subject) => subject.key)))
    ].filter((key) => /^\d+$/.test(key));
    const [folded, grounds] = await Promise.all([
      api.memoriesNamed(props.brainId, { supersededBy: ids.join(",") }),
      episodeIds.length === 0 ? Promise.resolve([]) : api.memoriesNamed(props.brainId, { ids: episodeIds.join(",") })
    ]);

    versions.value = lineage;
    merged.value = folded.filter((row) => row.lineageId !== found.lineageId);
    groundRows.value = grounds;
    chosen.value = chosen.value !== null && ids.includes(chosen.value) ? chosen.value : found.id;
  } catch (error) {
    island.fail(error, t("brain.error.read"));
  }
}

watch(() => [props.brainId, props.memoryId, props.changes], () => void read(), { immediate: true });

const span = computed(() => {
  const moments = [
    ...(versions.value ?? []).flatMap((version) => [version.validFrom, version.validTo ?? now]),
    ...merged.value.map((row) => row.validFrom),
    ...groundRows.value.map((row) => row.validFrom)
  ];
  const from = Math.min(...moments);
  const to = Math.max(...moments, from + 1);

  return { from, to };
});

function x(at: number): number {
  const { from, to } = span.value;

  return PAD + ((at - from) / (to - from)) * (WIDTH - 2 * PAD);
}

function colour(status: MemoryStatus): string {
  switch (status) {
    case "active":
      return "var(--ui-success)";
    case "pending":
      return "var(--ui-warning)";
    case "invalidated":
    case "retracted":
      return "var(--ui-error)";
    case "done":
      return "var(--ui-info)";
    default:
      return "var(--ui-text-dimmed)";
  }
}

// Who wrote a version, read off its writer's name: the screen's own writes are a person's, a session's are
// the agent's, the rest a tool's.
function markOf(version: Memory): "agent" | "tool" | "person" {
  if (version.actorName === api.SCREEN_ACTOR || version.actorName.startsWith("person")) return "person";

  return version.actorName.includes("session") ? "agent" : "tool";
}

const chosenVersion = computed(() => versions.value?.find((version) => version.id === chosen.value));

const change = computed(() => {
  const list = versions.value ?? [];
  const at = list.findIndex((version) => version.id === chosen.value);
  const current = list[at];

  if (current === undefined) return [];

  const before = list[at - 1];

  return before === undefined ? [{ text: current.text, change: "same" as const }] : wordChange(before.text, current.text);
});

function day(at: number): string {
  return new Date(at).toLocaleDateString();
}

function stamp(at: number): string {
  return new Date(at).toLocaleString();
}
</script>
