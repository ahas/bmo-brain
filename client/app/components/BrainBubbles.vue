<template>
  <!-- The bubble chart (docs/design.md, 「The screen」): one bubble per type or per layer, placed across by
       the group's mean importance and up by its mean use, sized by its count, so the important
       memories that are seldom used stand at the lower right. A bubble narrows the list to its
       group; a table shows the same figures. -->
  <div class="flex min-h-0 flex-1 flex-col">
    <BrainHeading v-bind="heading" :count="total">
      <template #tools>
        <span class="text-sm text-muted">{{ t("brain.label.group_by") }}</span>
        <TabSwitch :items="byItems" :model-value="by" :label="t('brain.label.group_by')" @update:model-value="emit('by', $event)" />
        <UTooltip :text="asTable ? t('brain.view.bubbles') : t('brain.label.as_table')">
          <UButton size="sm" square :icon="asTable ? 'i-material-symbols-bubble-chart-outline' : 'i-material-symbols-table-outline'" color="neutral" variant="ghost" :aria-label="asTable ? t('brain.view.bubbles') : t('brain.label.as_table')" @click="asTable = !asTable" />
        </UTooltip>
      </template>
    </BrainHeading>

    <div class="min-h-0 flex-1 overflow-auto p-3">
      <p v-if="groups.length === 0" class="p-6 text-center text-sm text-muted">{{ t("brain.status.nothing") }}</p>
      <table v-else-if="asTable" class="w-full text-sm">
        <thead class="text-xs text-muted">
          <tr>
            <th class="py-1 text-start">{{ by === "type" ? t("brain.field.type") : t("brain.field.layer") }}</th>
            <th class="py-1 text-end">{{ t("brain.label.count") }}</th>
            <th class="py-1 text-end">{{ t("brain.field.importance") }}</th>
            <th class="py-1 text-end">{{ t("brain.label.uses") }}</th>
            <th class="py-1 text-end">{{ t("brain.label.last_hit") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="group in groups" :key="group.key" class="cursor-pointer border-t border-default hover:bg-elevated" @click="narrow(group.key)">
            <td class="py-1"><span class="me-2 inline-block size-2 rounded-full" :style="{ background: group.colour }" />{{ group.name }}</td>
            <td class="py-1 text-end tabular-nums">{{ group.count }}</td>
            <td class="py-1 text-end tabular-nums">{{ group.importance.toFixed(1) }}</td>
            <td class="py-1 text-end tabular-nums">{{ group.uses.toFixed(1) }}</td>
            <td class="py-1 text-end tabular-nums">{{ group.lastUsedAt === null ? "–" : stamp(group.lastUsedAt) }}</td>
          </tr>
        </tbody>
      </table>
      <svg v-else :viewBox="`0 0 ${WIDTH} ${HEIGHT}`" class="h-auto max-h-full w-full" role="img" :aria-label="t('brain.view.bubbles')">
        <line :x1="PAD" :y1="HEIGHT - PAD" :x2="WIDTH - PAD" :y2="HEIGHT - PAD" class="stroke-(--ui-border-accented)" />
        <line :x1="PAD" :y1="PAD" :x2="PAD" :y2="HEIGHT - PAD" class="stroke-(--ui-border-accented)" />
        <text :x="WIDTH - PAD" :y="HEIGHT - 8" text-anchor="end" class="fill-(--ui-text-muted) text-[11px]">{{ t("brain.field.importance") }} →</text>
        <text :x="8" :y="PAD - 8" class="fill-(--ui-text-muted) text-[11px]">↑ {{ t("brain.label.uses") }}</text>
        <g v-for="group in placed" :key="group.key" class="cursor-pointer" @click="narrow(group.key)">
          <circle :cx="group.x" :cy="group.y" :r="group.r" :fill="group.colour" fill-opacity="0.55" :stroke="group.colour" class="transition-all duration-300 motion-reduce:transition-none" />
          <text :x="group.x" :y="group.y + 4" text-anchor="middle" class="pointer-events-none fill-(--ui-text-highlighted) text-[11px]">{{ group.name }}</text>
          <title>{{ group.name }} · {{ group.count }} · {{ group.importance.toFixed(1) }} · {{ group.uses.toFixed(1) }}</title>
        </g>
      </svg>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useIsland } from "bmo-ui/app/domain/island";
import BrainHeading from "./BrainHeading.vue";
import * as api from "../domain/brain-api";
import { groupColour } from "../domain/brain-colour";
import { typeName } from "../domain/brain-names";

import type { Layer, MemoryFilter, Tally } from "../domain/brain-api";
import type { Heading } from "../domain/brain-heading";
import type { Place } from "../domain/brain-place";

const props = defineProps<{ brainId: number; heading: Heading; by: "type" | "layer"; layers: Layer[]; filter: MemoryFilter; changes: number }>();

const emit = defineEmits<{ by: [by: "type" | "layer"]; narrow: [patch: Partial<Place>] }>();

const { t, te } = useI18n();
const island = useIsland(t);

const WIDTH = 640;
const HEIGHT = 420;
const PAD = 36;
// The room the largest bubble takes beyond its centre, kept clear inside the axes.
const ROOM = 48;

const tallied = ref<Tally[]>([]);
const asTable = ref(false);

const byItems = computed(() => [
  { value: "type" as const, label: t("brain.field.type") },
  { value: "layer" as const, label: t("brain.field.layer") }
]);

async function read(): Promise<void> {
  try {
    tallied.value = await api.tallies(props.brainId, props.by, props.filter);
  } catch (error) {
    island.fail(error, t("brain.error.read"));
  }
}

watch(() => [props.brainId, props.by, JSON.stringify(props.filter), props.changes], () => void read(), { immediate: true });

const total = computed(() => tallied.value.reduce((sum, tally) => sum + tally.count, 0));

const groups = computed(() =>
  tallied.value.map((tally) => {
    const key = tally.group ?? "";
    const name =
      props.by === "type" ? typeName(t, te, key) : tally.group === null ? t("brain.label.unfiled") : (props.layers.find((layer) => String(layer.id) === key)?.name ?? `#${key}`);

    return {
      key,
      name,
      count: tally.count,
      importance: tally.importance,
      uses: tally.useCount,
      lastUsedAt: tally.lastUsedAt ?? null,
      colour: groupColour(name)
    };
  })
);

const placed = computed(() => {
  const most = Math.max(1, ...groups.value.map((group) => group.count));
  const uses = groups.value.map((group) => group.uses);
  const low = Math.min(0, ...uses);
  const high = Math.max(low + 1, ...uses);

  return groups.value.map((group) => ({
    ...group,
    x: PAD + ROOM + ((group.importance - 1) / 9) * (WIDTH - 2 * (PAD + ROOM)),
    y: HEIGHT - PAD - ROOM - ((group.uses - low) / (high - low)) * (HEIGHT - 2 * (PAD + ROOM)),
    r: 8 + Math.sqrt(group.count / most) * 40
  }));
});

function stamp(at: number): string {
  return new Date(at).toLocaleString();
}

function narrow(key: string): void {
  if (props.by === "type") emit("narrow", { type: key });
  else emit("narrow", { layer: key === "" ? "unfiled" : Number(key) });
}
</script>
