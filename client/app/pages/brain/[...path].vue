<template>
  <!-- The brain screen (docs/design.md, 「The screen」): one brain at a time. Where there is room the index,
       the view and the memory open stand side by side behind splitters, each scrolling on its own; on a
       phone the view is the body, the index rises from the bottom edge and a memory stands over the view.
       The brain, the view, the filters and the memory open are the address. -->
  <UDashboardPanel id="brain" :ui="{ body: 'p-0 sm:p-0 gap-0 sm:gap-0 overflow-y-hidden [&>*]:max-w-none' }">
    <template #header>
      <UDashboardNavbar :title="t('brain.title.screen')" icon="i-material-symbols-neurology-outline" />
    </template>

    <template #body>
      <div v-if="brainList !== null && brainList.length === 0" class="p-6 text-center text-sm text-muted">
        {{ t("brain.status.no_brains") }}
      </div>

      <div v-else class="flex min-h-0 flex-1 flex-col">
        <ToolBar :label="t('brain.label.toolbar')">
          <div>
            <USelect
              :model-value="place.brainId ?? undefined"
              :items="brainItems"
              :placeholder="t('brain.label.brain')"
              :aria-label="t('brain.label.brain')"
              variant="ghost"
              class="min-w-36"
              @update:model-value="chooseBrain(Number($event))"
            />
          </div>
          <div v-if="narrow">
            <UButton icon="i-material-symbols-toc" color="neutral" variant="ghost" :aria-label="t('brain.action.index')" @click="indexOpen = true" />
          </div>
          <TabSwitch
            :items="viewItems"
            :model-value="place.view"
            :label="t('brain.label.view')"
            :compact="narrow"
            @update:model-value="go({ view: $event })"
          />
          <div>
            <UInput
              :model-value="place.q"
              icon="i-material-symbols-search"
              :placeholder="t('brain.label.search')"
              variant="none"
              class="w-40"
              @change="go({ q: ($event.target as HTMLInputElement).value, memory: null })"
            />
          </div>
          <div>
            <USelect
              :model-value="place.status"
              :items="statusItems"
              :aria-label="t('brain.label.status')"
              variant="ghost"
              @update:model-value="go({ status: $event as StatusChoice })"
            />
          </div>
          <div>
            <USelect
              :model-value="place.type"
              :items="typeItems"
              :aria-label="t('brain.label.type')"
              variant="ghost"
              @update:model-value="go({ type: String($event) })"
            />
          </div>
          <div>
            <USelect
              :model-value="place.order"
              :items="orderItems"
              :aria-label="t('brain.label.order')"
              variant="ghost"
              @update:model-value="go({ order: $event as Order })"
            />
          </div>
          <div>
            <UPopover>
              <UButton icon="i-material-symbols-filter-list" color="neutral" variant="ghost" :aria-label="t('brain.action.filters')" />
              <template #content>
                <div class="flex w-72 flex-col gap-3 p-3">
                  <UFormField :label="t('brain.field.key')">
                    <UInput :model-value="place.key" class="w-full" @change="go({ key: ($event.target as HTMLInputElement).value })" />
                  </UFormField>
                  <UFormField :label="t('brain.field.label')" :hint="t('brain.hint.label')">
                    <UInput :model-value="place.label" class="w-full" @change="go({ label: ($event.target as HTMLInputElement).value })" />
                  </UFormField>
                </div>
              </template>
            </UPopover>
          </div>
          <div v-if="place.layer !== null || place.subject !== null" class="gap-1">
            <UBadge
              v-if="place.layer !== null"
              color="neutral"
              variant="subtle"
              :label="layerName(place.layer)"
              trailing-icon="i-material-symbols-close"
              class="cursor-pointer"
              @click="go({ layer: null })"
            />
            <UBadge
              v-if="place.subject !== null"
              color="neutral"
              variant="subtle"
              :label="subjectName(place.subject)"
              trailing-icon="i-material-symbols-close"
              class="cursor-pointer"
              @click="go({ subject: null })"
            />
          </div>
          <div v-if="reader.offered" class="ms-auto">
            <UButton
              icon="i-material-symbols-translate"
              :label="narrow ? undefined : t('brain.action.reading')"
              :aria-label="t('brain.action.reading')"
              :aria-pressed="translating"
              color="neutral"
              :variant="translating ? 'soft' : 'ghost'"
              @click="reader.choose(translating ? ENGLISH : reader.home)"
            />
          </div>
          <div :class="{ 'ms-auto': !reader.offered }">
            <UButton icon="i-material-symbols-school-outline" :label="narrow ? undefined : t('brain.action.teach')" color="neutral" variant="ghost" :aria-label="t('brain.action.teach')" :disabled="place.brainId === null" @click="teachOpen = true" />
          </div>
        </ToolBar>

        <p v-if="translating && reader.failed.value" role="status" class="border-b border-default px-3 py-1.5 text-sm text-warning">{{ t("brain.error.translate_read") }}</p>

        <div v-if="narrow" class="relative flex min-h-0 flex-1 flex-col">
          <div class="flex min-h-0 flex-1 flex-col" :inert="place.memory !== null">
            <component :is="viewPart" v-bind="viewProps" />
          </div>
          <div v-if="place.memory !== null" class="absolute inset-0 z-30 flex min-h-0 flex-col bg-default">
            <BrainRow v-bind="rowProps" :narrow="true" @close="go({ memory: null })" @open="openMemory" @subject="go({ subject: $event, memory: null })" @layer="go({ layer: $event, memory: null })" @changed="reread" @timeline="go({ view: 'timeline' })" />
          </div>
          <UDrawer v-model:open="indexOpen" :title="t('brain.action.index')" :ui="FULL_SHEET">
            <template #body>
              <BrainIndex v-bind="indexProps" @choose="indexOpen = false; choose($event)" @changed="reread" />
            </template>
          </UDrawer>
        </div>

        <USplitter
          v-else
          id="brain-splitter"
          auto-save-id="bmo-brain-splitter"
          :items="SIDES"
          :ui="{ handle: SPLIT_HANDLE }"
          class="min-h-0 flex-1"
        >
          <template #index>
            <div class="flex min-h-0 min-w-0 flex-1 flex-col">
              <BrainIndex v-bind="indexProps" @choose="choose" @changed="reread" />
            </div>
          </template>
          <template #view>
            <div class="flex min-h-0 min-w-0 flex-1 flex-col">
              <component :is="viewPart" v-bind="viewProps" />
            </div>
          </template>
          <template #row>
            <div class="flex min-h-0 min-w-0 flex-1 flex-col">
              <BrainRow v-if="place.memory !== null" v-bind="rowProps" :narrow="false" @close="go({ memory: null })" @open="openMemory" @subject="go({ subject: $event, memory: null })" @layer="go({ layer: $event, memory: null })" @changed="reread" @timeline="go({ view: 'timeline' })" />
              <p v-else class="p-6 text-center text-sm text-muted">{{ t("brain.status.no_memory_open") }}</p>
            </div>
          </template>
        </USplitter>

        <BrainTeach
          v-if="place.brainId !== null"
          v-model:open="teachOpen"
          :brain-id="place.brainId"
          :layers="layerList"
          :narrow="narrow"
          @taught="taught"
        />
      </div>
    </template>
  </UDashboardPanel>
</template>

<script setup lang="ts">
import { useIsland } from "bmo-ui/app/domain/island";
import { useNarrow } from "bmo-ui/app/domain/screen";
import { FULL_SHEET, SPLIT_HANDLE } from "bmo-ui/app/domain/side-parts";

import BrainBook from "../../components/BrainBook.vue";
import BrainBubbles from "../../components/BrainBubbles.vue";
import BrainIndex from "../../components/BrainIndex.vue";
import BrainList from "../../components/BrainList.vue";
import BrainMindmap from "../../components/BrainMindmap.vue";
import BrainRow from "../../components/BrainRow.vue";
import BrainTeach from "../../components/BrainTeach.vue";
import BrainTimeline from "../../components/BrainTimeline.vue";
import * as api from "../../domain/brain-api";
import { EVERY_TYPE, EVERY_TYPE_BUT_EPISODES, ORDERS, STATUS_CHOICES, filterOf, orderOf, pathOf, placeOf, queryOf } from "../../domain/brain-place";
import { ENGLISH, useReading } from "../../domain/brain-reading";
import { useBrainStream } from "../../domain/brain-stream";

import type { Brain, Layer, Memory, Subject, Tally, TypeCount } from "../../domain/brain-api";
import type { Order, Place, StatusChoice, View } from "../../domain/brain-place";
import type { SplitterItem } from "@nuxt/ui";

// The screen is one screen for every place inside it, so a memory opened or a filter changed reads again
// what it needs and never builds the screen anew.
definePageMeta({ key: () => "brain" });

const SIDES: SplitterItem[] = [
  { slot: "index", minSize: 12, defaultSize: 20, collapsible: true, collapsedSize: 0 },
  { slot: "view", minSize: 30, defaultSize: 48 },
  { slot: "row", minSize: 20, defaultSize: 32, collapsible: true, collapsedSize: 0 }
];

const PAGE = 50;

const route = useRoute();
const router = useRouter();
const { t } = useI18n();
const island = useIsland(t);
const narrow = useNarrow();
// The reading language (docs/design.md, 「The screen」): the console's own where a translator is named.
const reader = useReading();
const translating = computed(() => reader.language.value !== ENGLISH);

const brainList = ref<Brain[] | null>(null);
const layerList = ref<Layer[]>([]);
const subjectList = ref<Subject[]>([]);
const typeList = ref<TypeCount[]>([]);
const layerCounts = ref<Tally[]>([]);
const subjectCounts = ref<Tally[]>([]);
const statusCounts = ref<Tally[]>([]);
const typeCounts = ref<Tally[]>([]);
const memories = ref<Memory[] | null>(null);
const more = ref(false);
const loadingMore = ref(false);
const listFailure = ref("");
const indexOpen = ref(false);
const teachOpen = ref(false);
// Bumped whenever the brain says something changed, so the parts that read on their own read again.
const changes = ref(0);

const place = computed<Place>(() => {
  const path = route.params.path;
  const segments = Array.isArray(path) ? path : typeof path === "string" && path !== "" ? path.split("/") : [];

  return placeOf(segments, route.query);
});

function go(patch: Partial<Place>): void {
  const next = { ...place.value, ...patch };

  void router.push({ path: pathOf(next), query: queryOf(next) });
}

function chooseBrain(brainId: number): void {
  void router.push({ path: pathOf({ ...place.value, brainId, layer: null, subject: null, memory: null }) });
}

function choose(patch: Partial<Place>): void {
  go({ ...patch, memory: null });
}

function openMemory(id: number): void {
  go({ memory: id });
}

const brainItems = computed(() =>
  (brainList.value ?? []).map((brain) => ({ label: brain.name || brain.agentKey, value: brain.id }))
);

const viewItems = computed(() =>
  (
    [
      ["list", "i-material-symbols-list"],
      ["map", "i-material-symbols-hub-outline"],
      ["bubbles", "i-material-symbols-bubble-chart-outline"],
      ["timeline", "i-material-symbols-timeline"],
      ["book", "i-material-symbols-menu-book-outline"]
    ] as const
  ).map(([value, icon]) => ({ value: value as View, label: t(`brain.view.${value}`), icon }))
);

function countOf(tallied: Tally[], groups: string[]): number {
  return tallied.filter((tally) => tally.group !== null && groups.includes(tally.group)).reduce((sum, tally) => sum + tally.count, 0);
}

const statusItems = computed(() =>
  STATUS_CHOICES.map((choice) => {
    const all = statusCounts.value.reduce((sum, tally) => sum + tally.count, 0);
    const count = choice === "all" ? all : choice === "live" ? countOf(statusCounts.value, ["active", "pending"]) : countOf(statusCounts.value, [choice]);

    return { value: choice, label: `${t(`brain.status_choice.${choice}`)} · ${count}` };
  })
);

const typeItems = computed(() => {
  const all = typeCounts.value.reduce((sum, tally) => sum + tally.count, 0);
  const episodes = countOf(typeCounts.value, ["episode"]);
  const named = typeList.value.map((type) => ({
    value: type.type,
    label: `${type.type} · ${countOf(typeCounts.value, [type.type])}`
  }));

  return [
    { value: EVERY_TYPE_BUT_EPISODES, label: `${t("brain.type_choice.but_episodes")} · ${all - episodes}` },
    { value: EVERY_TYPE, label: `${t("brain.type_choice.every")} · ${all}` },
    ...named
  ];
});

const orderItems = computed(() => ORDERS.map((order) => ({ value: order, label: t(`brain.order.${order.replace("-", "desc_")}`) })));

function layerName(layer: number | "unfiled"): string {
  if (layer === "unfiled") return t("brain.label.unfiled");

  return layerList.value.find((found) => found.id === layer)?.name ?? `#${layer}`;
}

function subjectName(id: number): string {
  const subject = subjectList.value.find((found) => found.id === id);

  return subject === undefined ? `#${id}` : subject.name ?? `${subject.type} ${subject.key}`;
}

// The brains, read once; a screen opened with no brain stands on the first.
async function readBrains(): Promise<void> {
  try {
    brainList.value = await api.brains();
  } catch (error) {
    brainList.value = [];
    island.fail(error, t("brain.error.read"));
    return;
  }

  const first = brainList.value[0];

  if (place.value.brainId === null && first !== undefined) {
    void router.replace({ path: pathOf({ ...place.value, brainId: first.id }), query: route.query });
  }
}

// The index: the layers, the subjects and the types, with their live counts.
async function readIndex(brainId: number): Promise<void> {
  const live = { status: "active,pending" };

  try {
    const [layers, subjects, types, byLayer, bySubject] = await Promise.all([
      api.layers(brainId),
      api.subjects(brainId),
      api.types(brainId),
      api.tallies(brainId, "layer", live),
      api.tallies(brainId, "subject", live)
    ]);

    layerList.value = layers;
    subjectList.value = subjects;
    typeList.value = types.filter((type) => type.count > 0);
    layerCounts.value = byLayer;
    subjectCounts.value = bySubject;
  } catch (error) {
    island.fail(error, t("brain.error.read"));
  }
}

// The counts beside the status and type choices, each under every filter but its own.
async function readCounts(brainId: number): Promise<void> {
  try {
    const [byStatus, byType] = await Promise.all([
      api.tallies(brainId, "status", filterOf(place.value, "status")),
      api.tallies(brainId, "type", filterOf(place.value, "type"))
    ]);

    statusCounts.value = byStatus;
    typeCounts.value = byType;
  } catch (error) {
    island.fail(error, t("brain.error.read"));
  }
}

// The list's first page, or as many pages as were read already where the list is read again.
async function readList(brainId: number, keepLength = false): Promise<void> {
  const wanted = keepLength && memories.value !== null ? Math.max(PAGE, memories.value.length) : PAGE;

  try {
    const page = await api.memoryPage(brainId, filterOf(place.value), orderOf(place.value), null, Math.min(wanted, 500));

    memories.value = page.items;
    more.value = page.more;
    listFailure.value = "";
  } catch (error) {
    listFailure.value = t("brain.error.read");
    island.fail(error, t("brain.error.read"));
  }
}

async function readMore(): Promise<void> {
  const brainId = place.value.brainId;
  const last = memories.value?.[memories.value.length - 1];

  if (brainId === null || last === undefined || loadingMore.value) return;

  loadingMore.value = true;

  try {
    const page = await api.memoryPage(brainId, filterOf(place.value), orderOf(place.value), last.id, PAGE);

    memories.value = [...(memories.value ?? []), ...page.items];
    more.value = page.more;
  } catch (error) {
    island.fail(error, t("brain.error.read"));
  } finally {
    loadingMore.value = false;
  }
}

// What the brain said changed, read again where it stands; the list keeps the pages it had read.
function reread(): void {
  const brainId = place.value.brainId;

  changes.value++;

  if (brainId === null) return;

  void readIndex(brainId);
  void readCounts(brainId);
  void readList(brainId, true);
}

function taught(memory: Memory | null): void {
  reread();

  if (memory !== null) openMemory(memory.id);
}

const listKey = computed(() => JSON.stringify([place.value.brainId, filterOf(place.value), orderOf(place.value)]));

watch(
  () => place.value.brainId,
  (brainId) => {
    if (brainId === null) return;

    memories.value = null;
    void readIndex(brainId);
  },
  { immediate: true }
);

watch(
  listKey,
  () => {
    const brainId = place.value.brainId;

    if (brainId === null) return;

    void readList(brainId);
    void readCounts(brainId);
  },
  { immediate: true }
);

useBrainStream(() => place.value.brainId, reread);

onMounted(() => void readBrains());

const subjectNames = computed(() => new Map(subjectList.value.map((subject) => [subject.id, subject.name ?? `${subject.type} ${subject.key}`])));

const indexProps = computed(() => ({
  brainId: place.value.brainId,
  layers: layerList.value,
  layerCounts: layerCounts.value,
  subjects: subjectList.value,
  subjectCounts: subjectCounts.value,
  types: typeList.value,
  place: place.value
}));

const rowProps = computed(() => ({
  brainId: place.value.brainId ?? 0,
  memoryId: place.value.memory ?? 0,
  layers: layerList.value,
  subjectNames: subjectNames.value,
  changes: changes.value
}));

const viewPart = computed(() => {
  switch (place.value.view) {
    case "map":
      return BrainMindmap;
    case "bubbles":
      return BrainBubbles;
    case "timeline":
      return BrainTimeline;
    case "book":
      return BrainBook;
    default:
      return BrainList;
  }
});

const viewProps = computed(() => {
  const brainId = place.value.brainId ?? 0;

  switch (place.value.view) {
    case "map":
      return {
        brainId,
        subjectId: place.value.subject,
        subjects: subjectList.value,
        layers: layerList.value,
        changes: changes.value,
        onOpen: openMemory,
        onCentre: (subject: number) => go({ subject }),
        onNarrow: (subject: number) => go({ subject, view: "list" })
      };
    case "bubbles":
      return {
        brainId,
        by: place.value.by,
        layers: layerList.value,
        status: place.value.status,
        changes: changes.value,
        onBy: (by: "type" | "layer") => go({ by }),
        onNarrow: (patch: Partial<Place>) => go({ ...patch, view: "list" })
      };
    case "timeline":
      return {
        brainId,
        memoryId: place.value.memory,
        changes: changes.value,
        onOpen: openMemory
      };
    case "book":
      return {
        brainId,
        layers: layerList.value,
        layerId: typeof place.value.layer === "number" ? place.value.layer : null,
        changes: changes.value,
        onLayer: (layer: number) => go({ layer }),
        onOpen: openMemory,
        onChanged: reread
      };
    default:
      return {
        memories: memories.value,
        more: more.value,
        loadingMore: loadingMore.value,
        failure: listFailure.value,
        layers: layerList.value,
        subjectNames: subjectNames.value,
        openId: place.value.memory,
        narrow: narrow.value,
        brainId,
        subjectId: place.value.subject,
        changes: changes.value,
        onOpen: openMemory,
        onMore: readMore
      };
  }
});
</script>
