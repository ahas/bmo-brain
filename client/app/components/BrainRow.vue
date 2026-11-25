<template>
  <!-- One memory in one version (docs/design.md, 「The screen」): every field it has, its place in its
       lineage, the moves its status allows, and an edit saved as a new version. A superseded version opens
       under a band leading to the live one. An org brain's memory is read alone: no edit and no move stands. -->
  <div class="flex min-h-0 flex-1 flex-col">
    <div class="flex shrink-0 items-center gap-1 border-b border-default px-1 py-1">
      <UTooltip :text="t('brain.action.close_memory')">
        <UButton size="sm" square :icon="narrow ? 'i-material-symbols-arrow-back' : 'i-material-symbols-close'" color="neutral" variant="ghost" :aria-label="t('brain.action.close_memory')" @click="emit('close')" />
      </UTooltip>
      <h2 class="min-w-0 flex-1 truncate text-base font-semibold">
        {{ memory ? t("brain.label.memory_head", { id: memoryId, type: typeName(t, te, memory.type) }) : `#${memoryId}` }}
      </h2>
      <UTooltip v-if="memory && editable" :text="t('brain.action.edit')">
        <UButton size="sm" square icon="i-material-symbols-edit-outline" color="neutral" variant="ghost" :aria-label="t('brain.action.edit')" @click="startEdit" />
      </UTooltip>
      <UTooltip :text="t('brain.action.all_versions')">
        <UButton size="sm" square icon="i-material-symbols-timeline" color="neutral" variant="ghost" :aria-label="t('brain.action.all_versions')" @click="emit('timeline')" />
      </UTooltip>
    </div>

    <ContentSkeleton v-if="memory === null && failure === ''" class="p-4" :rows="6" list-class="flex flex-col gap-3" :label="t('brain.status.loading')">
      <SkeletonBar class="h-5 w-full" />
    </ContentSkeleton>
    <p v-else-if="memory === null" class="p-4 text-sm text-error">{{ failure }}</p>

    <div v-else class="min-h-0 flex-1 overflow-y-auto pb-(--bmo-create-room)">
      <div v-if="memory.status === 'superseded' && live" class="flex items-center gap-2 border-b border-default bg-elevated px-4 py-2 text-sm">
        <UIcon name="i-material-symbols-history" class="size-4 shrink-0" />
        <span class="min-w-0 flex-1">{{ t("brain.status.superseded_band") }}</span>
        <UButton size="xs" color="neutral" variant="outline" :label="t('brain.action.open_live')" @click="emit('open', live.id)" />
      </div>

      <!-- What a service of the deployment left stands read-only here: the brain keeps it from every other
           writer, and the line says whose it is (docs/design.md, 「What a holder leaves」). -->
      <div v-if="heldBy !== null" class="flex items-center gap-2 border-b border-default px-4 py-2" data-held-note>
        <UIcon name="i-material-symbols-lock-outline" class="size-4 shrink-0 text-muted" aria-hidden="true" />
        <div class="min-w-0 flex-1">
          <UBadge color="neutral" variant="soft" size="sm">{{ t("brain.label.held") }} · <span class="font-mono">{{ heldBy }}</span></UBadge>
          <p class="mt-1 text-sm text-muted">{{ t("brain.hint.held") }}</p>
        </div>
      </div>

      <div class="flex flex-col gap-4 p-4">
        <div v-if="!editing">
          <BrainText :text="memory.text" text-class="text-base whitespace-pre-wrap select-text" />
        </div>
        <div v-else class="flex flex-col gap-3">
          <UFormField :label="t('brain.field.text')" :help="memory.type === 'episode' ? t('brain.hint.episode') : undefined">
            <UTextarea v-model="draft.text" :rows="4" autoresize class="w-full" :disabled="memory.type === 'episode'" />
          </UFormField>
          <UFormField v-if="draftWords.english.value !== null" :label="t('brain.label.translated_english')" :help="t('brain.hint.translated_english')">
            <UTextarea v-model="draftWords.english.value" :rows="3" autoresize class="w-full" />
          </UFormField>
          <p v-if="draftWords.fault.value !== null" class="text-sm text-warning">{{ t(faultKey(draftWords.fault.value)) }}</p>
          <div class="grid grid-cols-2 gap-3">
            <UFormField :label="t('brain.field.type')">
              <UInput v-model="draft.type" class="w-full" />
            </UFormField>
            <UFormField :label="t('brain.field.importance')">
              <UInput v-model.number="draft.importance" type="number" :min="1" :max="10" class="w-full" />
            </UFormField>
            <UFormField :label="t('brain.field.layer')">
              <ChoiceSelect v-model="draft.layer" :items="layerItems" :find="findLayers" value-key="value" class="w-full" />
            </UFormField>
            <UFormField :label="t('brain.field.position')">
              <UInput v-model.number="draft.position" type="number" class="w-full" />
            </UFormField>
          </div>
          <UFormField :label="t('brain.field.labels')" :hint="t('brain.hint.labels')">
            <UTextarea v-model="draft.labels" :rows="2" autoresize class="w-full font-mono text-sm" />
          </UFormField>
          <p v-if="memory.type === 'persona'" class="text-sm text-warning">{{ t("brain.hint.persona_cache") }}</p>
          <div class="flex justify-end gap-2">
            <UButton color="neutral" variant="ghost" :label="t('common.action.cancel')" @click="editing = false" />
            <UButton data-edit-save :label="t('common.action.save')" :loading="busy || draftWords.busy.value" @click="saveEdit" />
          </div>
        </div>

        <div v-if="!editing && !readOnly" class="flex flex-wrap gap-2">
          <UButton v-if="memory.status === 'pending' && heldBy === null" size="sm" icon="i-material-symbols-check" :label="t('brain.action.approve')" :loading="busy" @click="move('active')" />
          <!-- Recording a use is not a change of the row, so it stands for a held row as for any other. -->
          <UButton v-if="live && memory.id === live.id" size="sm" color="neutral" variant="outline" icon="i-material-symbols-thumb-up-outline" :label="t('brain.action.confirm')" :loading="busy" @click="confirm" />
          <template v-if="heldBy === null">
            <UButton v-if="isLive" size="sm" color="neutral" variant="outline" icon="i-material-symbols-task-alt" :label="t('brain.action.done')" @click="askMove('done')" />
            <UButton v-if="isLive" size="sm" color="error" variant="outline" icon="i-material-symbols-block" :label="t('brain.action.invalidate')" @click="askMove('invalidated')" />
            <UButton v-if="isLive" size="sm" color="error" variant="outline" icon="i-material-symbols-undo" :label="t('brain.action.retract')" @click="askMove('retracted')" />
            <UButton v-if="restorable" size="sm" color="neutral" variant="outline" icon="i-material-symbols-undo" :label="t('brain.action.restore')" :loading="busy" @click="restore" />
          </template>
        </div>

        <dl class="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2 text-sm [&>dt]:text-muted">
          <dt>{{ t("brain.field.status") }}</dt>
          <dd class="flex flex-wrap items-center gap-2">
            <BrainStatus :status="memory.status" />
            <span class="text-muted tabular-nums">{{ stamp(memory.validFrom) }} – {{ memory.validTo === null || memory.validTo === undefined ? "" : stamp(memory.validTo) }}</span>
          </dd>
          <template v-if="memory.reason">
            <dt>{{ t("brain.field.reason") }}</dt>
            <dd>{{ memory.reason }}</dd>
          </template>
          <template v-if="memory.key">
            <dt>{{ t("brain.field.key") }}</dt>
            <dd class="font-mono text-sm break-all">{{ memory.key }}</dd>
          </template>
          <template v-if="memory.layerId !== null && memory.layerId !== undefined">
            <dt>{{ t("brain.field.layer") }}</dt>
            <dd>
              <button type="button" class="text-primary hover:underline" @click="emit('layer', memory.layerId!)">{{ layerName(memory.layerId) }}</button>
              <span class="text-muted"> · {{ t("brain.field.position") }} {{ memory.position }}</span>
            </dd>
          </template>
          <template v-if="topics.length > 0">
            <dt>{{ t("brain.field.subjects") }}</dt>
            <!-- A subject's name takes the room the field has and no more: one too long for it is cut, and the cut line's
                 tooltip shows it whole (the design rules' lists, 「A line cut short」). -->
            <dd class="flex min-w-0 flex-wrap gap-1">
              <UBadge v-for="subject in topics" :key="subject.id" color="neutral" variant="outline" class="max-w-full cursor-pointer" @click="emit('subject', subject.id)">
                <span class="min-w-0 truncate">{{ subjectNames.get(subject.id) ?? subject.key }}</span>
                <span class="shrink-0 text-muted">{{ subjectTypeName(t, te, subject.type) }}</span>
              </UBadge>
            </dd>
          </template>
          <template v-if="grounds.length > 0 || source">
            <dt>{{ t("brain.field.grounds") }}</dt>
            <dd class="flex min-w-0 flex-wrap gap-1">
              <UBadge v-if="source" color="neutral" variant="soft" class="max-w-full" :label="t('brain.field.source') + ': ' + source" />
              <UBadge v-for="ground in grounds" :key="ground.id" color="neutral" variant="outline" class="max-w-full cursor-pointer" :label="`${typeName(t, te, 'episode')} ${ground.key}`" @click="openGround(ground.key)" />
            </dd>
          </template>
          <dt>{{ t("brain.field.lineage") }}</dt>
          <dd class="flex flex-wrap items-center gap-2">
            <span class="tabular-nums">{{ t("brain.label.version_of", { n: versionIndex + 1, of: versions.length }) }}</span>
            <UButton v-if="previous" size="xs" color="neutral" variant="ghost" icon="i-material-symbols-chevron-left" :label="t('brain.action.previous')" @click="emit('open', previous.id)" />
            <UButton v-if="next" size="xs" color="neutral" variant="ghost" trailing-icon="i-material-symbols-chevron-right" :label="t('brain.action.next')" @click="emit('open', next.id)" />
            <UButton v-if="memory.supersededBy" size="xs" color="neutral" variant="ghost" :label="t('brain.label.superseded_by', { id: memory.supersededBy })" @click="emit('open', memory.supersededBy!)" />
          </dd>
          <dt>{{ t("brain.field.use") }}</dt>
          <dd class="tabular-nums">
            {{ t("brain.label.use", { uses: memory.useCount, importance: memory.importance }) }}
            <template v-if="memory.lastUsedAt"> · {{ t("brain.label.last_hit") }} {{ stamp(memory.lastUsedAt) }}</template>
            <template v-if="memory.lastConfirmedAt"> · {{ t("brain.label.last_confirmed") }} {{ stamp(memory.lastConfirmedAt) }}</template>
          </dd>
          <template v-if="memory.consolidatedAt">
            <dt>{{ t("brain.field.consolidated") }}</dt>
            <dd class="tabular-nums">{{ stamp(memory.consolidatedAt) }}</dd>
          </template>
          <dt>{{ t("brain.field.writer") }}</dt>
          <dd>{{ memory.actorName }} <span class="text-muted tabular-nums">· {{ stamp(memory.createdAt) }}</span></dd>
          <template v-if="labelPairs.length > 0">
            <dt>{{ t("brain.field.labels") }}</dt>
            <dd class="font-mono text-sm">
              <div v-for="[name, value] in labelPairs" :key="name">{{ name }} = {{ value }}</div>
            </dd>
          </template>
        </dl>
      </div>
    </div>

    <UModal v-model:open="moving" :title="moveLabel">
      <template #body>
        <div class="flex flex-col gap-3">
          <UFormField :label="t('brain.field.reason')" :help="t('brain.hint.reason')">
            <UTextarea v-model="reason" :rows="2" autoresize class="w-full" />
          </UFormField>
          <UFormField v-if="reasonWords.english.value !== null" :label="t('brain.label.translated_english')" :help="t('brain.hint.translated_english')">
            <UTextarea v-model="reasonWords.english.value" :rows="2" autoresize class="w-full" />
          </UFormField>
          <p v-if="reasonWords.fault.value !== null" class="text-sm text-warning">{{ t(faultKey(reasonWords.fault.value)) }}</p>
          <UFormField v-if="moveTo === 'done'" :label="t('brain.field.ended_on')" :help="t('brain.hint.ended_on')">
            <UTextarea v-model="endedOn" :rows="2" autoresize class="w-full" />
          </UFormField>
          <UFormField v-if="moveTo === 'done' && endedWords.english.value !== null" :label="t('brain.label.translated_english')" :help="t('brain.hint.translated_english')">
            <UTextarea v-model="endedWords.english.value" :rows="2" autoresize class="w-full" />
          </UFormField>
          <p v-if="endedWords.fault.value !== null" class="text-sm text-warning">{{ t(faultKey(endedWords.fault.value)) }}</p>
        </div>
      </template>
      <template #footer>
        <div class="flex w-full justify-end gap-2">
          <UButton color="neutral" variant="ghost" :label="t('common.action.cancel')" @click="moving = false" />
          <UButton :color="moveTo === 'done' ? 'primary' : 'error'" :label="moveLabel" :loading="busy || endedWords.busy.value || reasonWords.busy.value" @click="move(moveTo)" />
        </div>
      </template>
    </UModal>
  </div>
</template>

<script setup lang="ts">
import { useIsland } from "bmo-ui/app/domain/island";
import { askQuestion } from "bmo-ui/app/domain/question";
import BrainStatus from "./BrainStatus.vue";
import BrainText from "./BrainText.vue";
import * as api from "../domain/brain-api";
import { subjectTypeName, typeName } from "../domain/brain-names";
import { faultKey, useEnglishWords } from "../domain/brain-reading";

import type { Layer, Memory, MemoryStatus } from "../domain/brain-api";

const props = defineProps<{
  brainId: number;
  memoryId: number;
  layers: Layer[];
  subjectNames: Map<number, string>;
  changes: number;
  narrow: boolean;
  // Whether the memory is an org brain's, which the screen reads and never writes (docs/design.md, 「The screen」).
  readOnly: boolean;
}>();

const emit = defineEmits<{
  close: [];
  open: [id: number];
  subject: [id: number];
  layer: [id: number];
  changed: [];
  timeline: [];
}>();

const { t, te } = useI18n();
const island = useIsland(t);

const memory = ref<Memory | null>(null);
const versions = ref<Memory[]>([]);
const failure = ref("");
const busy = ref(false);
const editing = ref(false);
const moving = ref(false);
const moveTo = ref<MemoryStatus>("done");
const reason = ref("");
const endedOn = ref("");

const NO_LAYER = "none";

const draft = reactive({ text: "", type: "", importance: 5, layer: NO_LAYER as string, position: 0, labels: "" });

// Words typed in the console's language are read into English and confirmed before a row holds them.
const draftWords = useEnglishWords(() => draft.text);
const endedWords = useEnglishWords(() => endedOn.value);
const reasonWords = useEnglishWords(() => reason.value);

async function read(): Promise<void> {
  failure.value = "";

  try {
    const found = await api.memory(props.brainId, props.memoryId);

    memory.value = found;
    versions.value = await api.lineage(props.brainId, found.lineageId);
  } catch (error) {
    memory.value = null;
    failure.value = t("brain.error.read");
    island.fail(error, t("brain.error.read"));
  }
}

watch(() => [props.brainId, props.memoryId], () => {
  memory.value = null;
  editing.value = false;
  void read();
}, { immediate: true });

watch(() => props.changes, () => {
  if (!editing.value) void read();
});

const versionIndex = computed(() => versions.value.findIndex((version) => version.id === props.memoryId));
const previous = computed(() => (versionIndex.value > 0 ? versions.value[versionIndex.value - 1] : undefined));
const next = computed(() => versions.value[versionIndex.value + 1]);
const newest = computed(() => versions.value[versions.value.length - 1]);
const live = computed(() => versions.value.find((version) => version.status === "active" || version.status === "pending"));
const isLive = computed(() => memory.value?.status === "active" || memory.value?.status === "pending");
// What a service of the deployment left standing in this brain: the screen reads it and ends none of it
// (docs/design.md, 「What a holder leaves」). Recording its use is the one act here a held row takes.
const heldBy = computed(() => memory.value?.heldBy ?? null);
const editable = computed(() => isLive.value && !props.readOnly && heldBy.value === null);
const restorable = computed(
  () =>
    memory.value !== null &&
    (memory.value.status === "done" || memory.value.status === "invalidated") &&
    newest.value?.id === memory.value.id
);

const topics = computed(() => memory.value?.subjects.filter((subject) => subject.type !== "episode") ?? []);
const grounds = computed(() => memory.value?.subjects.filter((subject) => subject.type === "episode") ?? []);
const source = computed(() => memory.value?.labels.source ?? null);
const labelPairs = computed(() => Object.entries(memory.value?.labels ?? {}).filter(([name]) => name !== "source"));

// The memory's own layer stands in the picker even where it lies past the pages the index had read.
const ownLayer = ref<Layer | null>(null);

watch(
  () => [props.brainId, props.memoryId, props.layers, memory.value?.layerId] as const,
  async () => {
    const id = memory.value?.layerId;

    if (id === null || id === undefined || props.layers.some((layer) => layer.id === id)) {
      ownLayer.value = null;
      return;
    }

    try {
      ownLayer.value = await api.layer(props.brainId, id);
    } catch {
      ownLayer.value = null;
    }
  },
  { immediate: true }
);

const layerItems = computed(() => [
  { value: NO_LAYER, label: t("brain.label.unfiled") },
  ...[...props.layers, ...(ownLayer.value === null ? [] : [ownLayer.value])]
    .filter((layer, index, all) => all.findIndex((one) => one.id === layer.id) === index)
    .map((layer) => ({ value: String(layer.id), label: layer.name }))
]);

// The layers the picker finds at the brain for the words typed, past the pages the index had read.
async function findLayers(words: string): Promise<{ value: string; label: string }[]> {
  try {
    const page = await api.layerPage(props.brainId, { q: words.trim(), limit: 50 });

    return [
      { value: NO_LAYER, label: t("brain.label.unfiled") },
      ...page.items.map((layer) => ({ value: String(layer.id), label: layer.name }))
    ];
  } catch {
    return [];
  }
}

function layerName(id: number): string {
  return props.layers.find((layer) => layer.id === id)?.name ?? (ownLayer.value?.id === id ? ownLayer.value.name : `#${id}`);
}

function stamp(at: number): string {
  return new Date(at).toLocaleString();
}

// An episode ground is a subject keyed by the episode's id.
function openGround(key: string): void {
  if (/^\d+$/.test(key)) emit("open", Number(key));
}

function startEdit(): void {
  const found = memory.value;

  if (found === null) return;

  draft.text = found.text;
  draft.type = found.type;
  draft.importance = found.importance;
  draft.layer = found.layerId === null || found.layerId === undefined ? NO_LAYER : String(found.layerId);
  draft.position = found.position;
  draft.labels = Object.entries(found.labels).map(([name, value]) => `${name}=${value}`).join("\n");
  editing.value = true;
}

function labelsOf(text: string): Record<string, string> {
  return Object.fromEntries(
    text
      .split("\n")
      .map((line) => line.trim())
      .filter((line) => line.includes("="))
      .map((line) => {
        const at = line.indexOf("=");

        return [line.slice(0, at).trim(), line.slice(at + 1).trim()];
      })
  );
}

async function saveEdit(): Promise<void> {
  const found = memory.value;

  if (found === null) return;

  // The text only where it changed: an unchanged text is the row's own English and is not asked for again.
  const text = draft.text === found.text ? null : await draftWords.ready();

  if (draft.text !== found.text && text === null) return;

  if (found.type === "persona" && !(await askQuestion({ title: t("brain.title.persona_save"), effect: t("brain.hint.persona_cache"), confirm: t("common.action.save") }))) return;

  busy.value = true;

  try {
    const written = await api.writeVersion(props.brainId, found.id, {
      text,
      type: draft.type === found.type ? null : draft.type,
      importance: draft.importance === found.importance ? null : draft.importance,
      layerId: draft.layer === NO_LAYER ? null : Number(draft.layer),
      position: draft.position === found.position ? null : draft.position,
      labels: labelsOf(draft.labels)
    });

    editing.value = false;
    emit("changed");
    emit("open", written.id);
  } catch (error) {
    // Two refusals both keep the words being written and read the row again: another writer moved the row
    // first, or a service of the deployment left this row and keeps it from every other writer.
    const refusal = api.isLocked(error) ? "brain.error.held" : api.isConflict(error) ? "brain.error.conflict" : null;

    if (refusal !== null) {
      island.say({ tone: "warning", title: t(refusal) });
      const kept = { ...draft };
      const english = draftWords.english.value;

      await read();
      Object.assign(draft, kept);
      await nextTick();
      draftWords.english.value = english;
    } else {
      island.fail(error, t("brain.error.save"));
    }
  } finally {
    busy.value = false;
  }
}

// The move asked for, by its action's name: a row is done, invalidated (it held and stopped) or retracted (it
// was never so).
const moveLabel = computed(() => {
  switch (moveTo.value) {
    case "done":
      return t("brain.action.done");
    case "retracted":
      return t("brain.action.retract");
    default:
      return t("brain.action.invalidate");
  }
});

function askMove(status: MemoryStatus): void {
  moveTo.value = status;
  reason.value = "";
  endedOn.value = "";
  moving.value = true;
}

async function move(status: MemoryStatus): Promise<void> {
  const found = memory.value;

  if (found === null) return;

  // The words it ended on and the reason, each English as typed or confirmed in English; both are read at once.
  const ending = status === "done" && endedOn.value.trim() !== "";
  const giving = reason.value.trim() !== "";
  const [words, why] = await Promise.all([ending ? endedWords.ready() : null, giving ? reasonWords.ready() : null]);

  if ((ending && words === null) || (giving && why === null)) return;

  busy.value = true;

  try {
    const moved = await api.moveMemory(props.brainId, found.id, status, why, words);

    moving.value = false;
    emit("changed");

    if (moved.id !== found.id) emit("open", moved.id);
    else await read();
  } catch (error) {
    island.fail(error, t("brain.error.save"));
  } finally {
    busy.value = false;
  }
}

async function restore(): Promise<void> {
  const found = memory.value;

  if (found === null) return;

  busy.value = true;

  try {
    const restored = await api.restoreMemory(props.brainId, found.id, null);

    emit("changed");
    emit("open", restored.id);
  } catch (error) {
    island.fail(error, t("brain.error.save"));
  } finally {
    busy.value = false;
  }
}

async function confirm(): Promise<void> {
  const found = memory.value;

  if (found === null) return;

  busy.value = true;

  try {
    await api.confirmMemory(props.brainId, found.id);
    island.say({ tone: "success", title: t("brain.status.confirmed") });
    await read();
  } catch (error) {
    island.fail(error, t("brain.error.save"));
  } finally {
    busy.value = false;
  }
}
</script>
