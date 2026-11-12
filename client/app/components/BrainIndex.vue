<template>
  <!-- The index (docs/design.md, 「The screen」): the layers in position order with their live counts and the
       unfiled last, the subjects grouped by type and called by their names, and the types in use, each tab with
       one line saying what it lists. Choosing one narrows the view to it, and choosing it again clears it; the
       chosen row stands highlighted and in sight, and where the view honours no choice the rows say so. A layer is added and its description changed here; a subject's
       description too. A row's actions are icon buttons at its end, shown while the row is hovered or
       focused and always on a coarse pointer; a row never grows a second line of buttons. -->
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
      <ul v-if="tab === 'layers'" class="flex flex-col py-1">
        <li v-for="layer in shownLayers" :key="layer.id" class="group flex items-stretch hover:bg-elevated" :class="{ 'bg-primary/10 shadow-[inset_3px_0_0_var(--ui-primary)]': place.layer === layer.id }" :data-chosen="place.layer === layer.id || undefined">
          <button
            type="button"
            class="flex min-w-0 flex-1 items-start gap-2 py-2 ps-3 text-start"
            :aria-pressed="place.layer === layer.id"
            :disabled="!usable"
            @click="emit('choose', { layer: place.layer === layer.id ? null : layer.id })"
          >
            <div class="min-w-0 flex-1">
              <div class="truncate text-sm font-medium">{{ layer.name }}</div>
              <div v-if="layer.description" class="truncate text-sm text-muted">{{ layer.description }}</div>
            </div>
            <UBadge color="neutral" variant="soft" size="sm"><RollingNumber :value="layerCount(String(layer.id))" /></UBadge>
          </button>
          <div class="flex items-center pe-2 ps-1 opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100 pointer-coarse:opacity-100">
            <UTooltip :text="t('brain.action.describe')">
              <UButton size="xs" color="neutral" variant="ghost" icon="i-material-symbols-edit-outline" :aria-label="t('brain.action.describe')" @click="describeLayer(layer)" />
            </UTooltip>
          </div>
        </li>
        <li>
          <button
            type="button"
            class="flex w-full items-center gap-2 px-3 py-2 text-start hover:bg-elevated"
            :class="{ 'bg-primary/10 shadow-[inset_3px_0_0_var(--ui-primary)]': place.layer === 'unfiled' }"
            :data-chosen="place.layer === 'unfiled' || undefined"
            :aria-pressed="place.layer === 'unfiled'"
            :disabled="!usable"
            @click="emit('choose', { layer: place.layer === 'unfiled' ? null : 'unfiled' })"
          >
            <span class="min-w-0 flex-1 truncate text-sm text-muted">{{ t("brain.label.unfiled") }}</span>
            <UBadge color="neutral" variant="soft" size="sm"><RollingNumber :value="layerCount(null)" /></UBadge>
          </button>
        </li>
        <li class="px-3 py-2">
          <UButton size="sm" color="neutral" variant="outline" icon="i-material-symbols-add" :label="t('brain.action.layer_add')" :disabled="brainId === null" @click="addLayer" />
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
                <UTooltip :text="t('brain.action.describe')">
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

    <UModal v-model:open="editing" :title="editTitle">
      <template #body>
        <div class="flex flex-col gap-3">
          <UFormField v-if="editKind === 'layer-new'" :label="t('brain.field.name')">
            <UInput v-model="editName" class="w-full" />
          </UFormField>
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

import type { Layer, Subject, Tally, TypeCount } from "../domain/brain-api";
import type { Place } from "../domain/brain-place";

const props = defineProps<{
  brainId: number | null;
  layers: Layer[];
  layerCounts: Tally[];
  subjects: Subject[];
  subjectCounts: Tally[];
  types: TypeCount[];
  place: Place;
}>();

const emit = defineEmits<{ choose: [patch: Partial<Place>]; changed: [] }>();

const { t, te } = useI18n();
const island = useIsland(t);

type Tab = "layers" | "subjects" | "types";

const tab = ref<Tab>("layers");
const words = ref("");
const scroller = ref<HTMLElement | null>(null);

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
const editKind = ref<"layer" | "layer-new" | "subject">("layer");
const editId = ref(0);
const editName = ref("");
const editDescription = ref("");
const editPosition = ref(0);

const editTitle = computed(() =>
  editKind.value === "layer-new" ? t("brain.action.layer_add") : t("brain.action.describe")
);

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

function addLayer(): void {
  editKind.value = "layer-new";
  editName.value = "";
  editDescription.value = "";
  editing.value = true;
}

async function save(): Promise<void> {
  if (props.brainId === null) return;

  saving.value = true;

  try {
    if (editKind.value === "layer-new") await api.addLayer(props.brainId, editName.value.trim(), editDescription.value);
    else if (editKind.value === "layer") await api.changeLayer(props.brainId, editId.value, { description: editDescription.value, position: editPosition.value });
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
