<template>
  <!-- The mind map (docs/design.md, 「The screen」): one subject at the centre, its live memories around it
       coloured by layer, the warmest first and the rest folded into one count, and outside them the other
       subjects those memories reach. A memory opens its row; an outer subject comes to the centre, every
       point moving to its new place at once. -->
  <div class="flex min-h-0 flex-1 flex-col">
    <BrainHeading v-bind="heading" :title="subjectId === null ? heading.title : `${heading.title} · ${nameOf(subjectId)}`" :count="subjectId === null ? null : memories.length" :notes="subjectId === null ? [...heading.notes, t('brain.heading.map_needs_subject')] : heading.notes" />
    <div v-if="subjectId === null" class="flex flex-col gap-2 p-4">
      <p class="text-sm text-muted">{{ t("brain.status.map_choose") }}</p>
      <div class="flex flex-wrap gap-1">
        <UButton v-for="subject in subjects.slice(0, 40)" :key="subject.id" size="xs" color="neutral" variant="outline" :label="subject.name ?? `${subjectTypeName(t, te, subject.type)} ${subject.key}`" @click="emit('centre', subject.id)" />
      </div>
    </div>
    <div v-else class="min-h-0 flex-1 overflow-auto p-2 pb-[calc(var(--spacing)*2+var(--bmo-create-room))]">
      <svg :viewBox="`${-SIZE / 2} ${-SIZE / 2} ${SIZE} ${SIZE}`" class="mx-auto h-full max-h-[48rem] w-full" role="img" :aria-label="t('brain.view.map')">
        <g v-for="point in points" :key="`l${point.key}`">
          <line :x1="point.from.x" :y1="point.from.y" :x2="point.x" :y2="point.y" class="stroke-(--ui-border-accented) transition-all duration-500 motion-reduce:transition-none" />
        </g>
        <g v-for="point in points" :key="point.key" class="cursor-pointer" @click="press(point)">
          <circle :cx="point.x" :cy="point.y" :r="point.r" :fill="point.colour" :fill-opacity="point.kind === 'memory' ? 0.8 : 0.25" :stroke="point.colour" class="transition-all duration-500 motion-reduce:transition-none" />
          <text :x="point.x" :y="point.y + point.r + 12" text-anchor="middle" class="pointer-events-none fill-(--ui-text) text-[11px] transition-all duration-500 motion-reduce:transition-none">{{ point.label }}</text>
          <title>{{ point.title }}</title>
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
import { subjectTypeName } from "../domain/brain-names";

import type { Layer, Memory, MemoryFilter, Subject } from "../domain/brain-api";
import type { Heading } from "../domain/brain-heading";

const props = defineProps<{
  brainId: number;
  heading: Heading;
  subjectId: number | null;
  filter: MemoryFilter;
  subjects: Subject[];
  layers: Layer[];
  changes: number;
}>();

const emit = defineEmits<{ open: [id: number]; centre: [id: number]; narrow: [id: number] }>();

const { t, te } = useI18n();
const island = useIsland(t);

const SIZE = 640;
const INNER = 160;
const OUTER = 280;
// How many memories stand around the centre before the rest fold into one count.
const SHOWN = 12;

const memories = ref<Memory[]>([]);

async function read(): Promise<void> {
  if (props.subjectId === null) return;

  try {
    const live = await api.memoriesNamed(props.brainId, { ...props.filter, subjectId: props.subjectId });

    memories.value = live.sort((a, b) => b.useCount - a.useCount || b.importance - a.importance);
  } catch (error) {
    island.fail(error, t("brain.error.read"));
  }
}

watch(() => [props.brainId, props.subjectId, JSON.stringify(props.filter), props.changes], () => void read(), { immediate: true });

interface Point {
  key: string;
  kind: "centre" | "memory" | "more" | "subject";
  id: number;
  x: number;
  y: number;
  r: number;
  colour: string;
  label: string;
  title: string;
  from: { x: number; y: number };
}

function nameOf(id: number): string {
  const subject = props.subjects.find((found) => found.id === id);

  return subject === undefined ? `#${id}` : (subject.name ?? `${subjectTypeName(t, te, subject.type)} ${subject.key}`);
}

function short(text: string): string {
  return text.length > 24 ? `${text.slice(0, 23)}…` : text;
}

const points = computed<Point[]>(() => {
  if (props.subjectId === null) return [];

  const centre = { x: 0, y: 0 };
  const shown = memories.value.slice(0, SHOWN);
  const folded = memories.value.length - shown.length;
  const ring = shown.length + (folded > 0 ? 1 : 0);
  const all: Point[] = [
    { key: `s${props.subjectId}`, kind: "centre", id: props.subjectId, ...centre, r: 22, colour: "var(--ui-primary)", label: nameOf(props.subjectId), title: nameOf(props.subjectId), from: centre }
  ];
  const reached = new Map<number, { x: number; y: number }>();

  shown.forEach((memory, index) => {
    const angle = (2 * Math.PI * index) / Math.max(1, ring) - Math.PI / 2;
    const at = { x: Math.cos(angle) * INNER, y: Math.sin(angle) * INNER };
    const layer = props.layers.find((found) => found.id === memory.layerId);

    all.push({
      key: `m${memory.id}`,
      kind: "memory",
      id: memory.id,
      ...at,
      r: 6 + memory.importance,
      colour: groupColour(layer?.name ?? "unfiled"),
      label: short(memory.text),
      title: memory.text,
      from: centre
    });

    for (const subject of memory.subjects) {
      if (subject.id !== props.subjectId && subject.type !== "episode" && !reached.has(subject.id)) reached.set(subject.id, at);
    }
  });

  if (folded > 0) {
    const angle = (2 * Math.PI * shown.length) / ring - Math.PI / 2;

    all.push({
      key: "more",
      kind: "more",
      id: props.subjectId,
      x: Math.cos(angle) * INNER,
      y: Math.sin(angle) * INNER,
      r: 14,
      colour: "var(--ui-text-dimmed)",
      label: t("brain.label.more_count", { n: folded }),
      title: t("brain.label.more_count", { n: folded }),
      from: centre
    });
  }

  [...reached.entries()].forEach(([id, from], index) => {
    const angle = (2 * Math.PI * index) / Math.max(1, reached.size) - Math.PI / 2 + Math.PI / Math.max(2, reached.size);

    all.push({
      key: `s${id}`,
      kind: "subject",
      id,
      x: Math.cos(angle) * OUTER,
      y: Math.sin(angle) * OUTER,
      r: 12,
      colour: "var(--ui-secondary)",
      label: short(nameOf(id)),
      title: nameOf(id),
      from
    });
  });

  return all;
});

function press(point: Point): void {
  if (point.kind === "memory") emit("open", point.id);
  else if (point.kind === "subject") emit("centre", point.id);
  else emit("narrow", point.id);
}
</script>
