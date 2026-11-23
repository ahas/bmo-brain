<template>
  <!-- The brain screen (docs/design.md, 「The screen」): one brain at a time. Where there is room the index,
       the view and the memory open stand side by side behind splitters, each scrolling on its own; on a
       phone the view is the body, the index rises from the bottom edge and a memory stands over the view.
       The brain, the view, the filters and the memory open are the address. -->
  <UDashboardPanel id="brain" :ui="{ body: 'p-0 sm:p-0 gap-0 sm:gap-0 overflow-y-hidden [&>*]:max-w-none' }">
    <!-- Under a console that offers its title bar the screen draws none of its own (bmo-ui's
         `docs/console-bundles.md`, "The title bar"). Either bar holds the name alone: the screen's actions and its
         create button stand at the end of its toolbar. -->
    <template v-if="bar === null" #header>
      <UDashboardNavbar :title="t('brain.title.screen')" icon="i-material-symbols-neurology-outline" />
    </template>

    <template #body>
      <div v-if="brainList !== null && brainList.length === 0" class="p-6 text-center text-sm text-muted">
        {{ t("brain.status.no_brains") }}
      </div>

      <div v-else class="flex min-h-0 flex-1 flex-col">
        <ToolBar ref="toolbar" :label="t('brain.label.toolbar')">
          <div v-if="!narrow">
            <span class="text-muted">{{ t("brain.label.brain") }}</span>
            <ChoiceSelect v-bind="brainSelect" size="sm" variant="ghost" class="min-w-20" />
          </div>
          <div v-if="narrow">
            <UTooltip :text="t('brain.action.index')">
              <UButton size="sm" square icon="i-material-symbols-toc" color="neutral" variant="ghost" :aria-label="t('brain.action.index')" @click="indexOpen = true" />
            </UTooltip>
          </div>
          <div>
            <span class="text-muted">{{ t("brain.label.view") }}</span>
            <ChoiceSelect
              size="sm"
              :model-value="place.view"
              :items="viewItems"
              value-key="value"
              :icon="viewItems.find((item) => item.value === place.view)?.icon"
              :aria-label="t('brain.label.view')"
              variant="ghost"
              class="min-w-24"
              @update:model-value="go({ view: $event as View })"
            />
          </div>
          <div v-if="!gathered">
            <UTooltip :text="unhonoured('q')" :disabled="honours(place.view, 'q')">
              <UInput v-bind="wordsInput" size="sm" icon="i-material-symbols-search" :placeholder="t('brain.label.search')" variant="none" class="w-28" />
            </UTooltip>
          </div>
          <div>
            <UPopover>
              <UTooltip :text="t('brain.action.filters')">
                <UButton size="sm" square icon="i-material-symbols-filter-list" color="neutral" variant="ghost" :aria-label="t('brain.action.filters')" :disabled="!honours(place.view, 'key')" />
              </UTooltip>
              <template #content>
                <div class="flex w-72 max-w-[calc(100vw-2rem)] flex-col gap-3 p-3">
                  <UFormField v-if="gathered" :label="t('brain.label.words')" :help="unhonoured('q')">
                    <UInput v-bind="wordsInput" icon="i-material-symbols-search" class="w-full" />
                  </UFormField>
                  <UFormField :label="t('brain.label.status')" :help="unhonoured('status')">
                    <ChoiceSelect
                      :model-value="place.status"
                      :items="statusItems"
                      value-key="value"
                      :aria-label="t('brain.label.status')"
                      :disabled="!honours(place.view, 'status')"
                      class="w-full"
                      @update:model-value="go({ status: $event as StatusChoice })"
                    />
                  </UFormField>
                  <UFormField :label="t('brain.label.type')" :help="unhonoured('type')">
                    <ChoiceSelect
                      :model-value="place.type"
                      :items="typeItems"
                      value-key="value"
                      :aria-label="t('brain.label.type')"
                      :disabled="!honours(place.view, 'type')"
                      class="w-full"
                      @update:model-value="go({ type: String($event) })"
                    />
                  </UFormField>
                  <UFormField :label="t('brain.label.order')" :help="unhonoured('order')">
                    <ChoiceSelect
                      :model-value="orderOf(place) as Order"
                      :items="orderItems"
                      value-key="value"
                      :aria-label="t('brain.label.order')"
                      :disabled="!honours(place.view, 'order')"
                      class="w-full"
                      @update:model-value="go({ order: $event as Order })"
                    />
                  </UFormField>
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
          <ActionGroup class="ms-auto" :actions="screenActions" />
          <!-- The screen's one create button, its last cell, and on a phone floating over the view instead; its
               press and its key drop the menu of what it makes. -->
          <UDropdownMenu :items="createItems" :disabled="place.brainId === null" :content="createMenu">
            <CreateButton ref="create" :class="{ 'ms-auto': screenActions.length === 0 }" :label="t('brain.action.create')" :disabled="place.brainId === null" />
          </UDropdownMenu>
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
              <UFormField :label="t('brain.label.brain')" class="px-3 pb-3">
                <ChoiceSelect v-bind="brainSelect" class="w-full" />
              </UFormField>
              <BrainIndex v-bind="indexProps" @choose="indexOpen = false; choose($event)" @changed="reread" />
            </template>
          </UDrawer>
        </div>

        <USplitter
          v-else
          id="brain-splitter"
          auto-save-id="bmo-brain-splitter"
          :key="place.memory === null ? 'two' : 'three'"
          :items="place.memory === null ? TWO_SIDES : SIDES"
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
          <template v-if="place.memory !== null" #row>
            <div class="flex min-h-0 min-w-0 flex-1 flex-col">
              <BrainRow v-bind="rowProps" :narrow="false" @close="go({ memory: null })" @open="openMemory" @subject="go({ subject: $event, memory: null })" @layer="go({ layer: $event, memory: null })" @changed="reread" @timeline="go({ view: 'timeline' })" />
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
        <BrainLayerAdd v-if="place.brainId !== null" v-model:open="layerAddOpen" :brain-id="place.brainId" @added="reread" />
      </div>
    </template>
  </UDashboardPanel>
</template>

<script setup lang="ts">
import { useBarGathers } from "bmo-ui/app/domain/actionbar";
import { useIsland } from "bmo-ui/app/domain/island";
import { useNarrow } from "bmo-ui/app/domain/screen";
import { FULL_SHEET, SPLIT_HANDLE } from "bmo-ui/app/domain/side-parts";

import BrainBook from "../../components/BrainBook.vue";
import BrainBubbles from "../../components/BrainBubbles.vue";
import BrainIndex from "../../components/BrainIndex.vue";
import BrainLayerAdd from "../../components/BrainLayerAdd.vue";
import BrainList from "../../components/BrainList.vue";
import BrainMindmap from "../../components/BrainMindmap.vue";
import BrainReview from "../../components/BrainReview.vue";
import BrainRow from "../../components/BrainRow.vue";
import BrainTeach from "../../components/BrainTeach.vue";
import BrainTimeline from "../../components/BrainTimeline.vue";
import * as api from "../../domain/brain-api";
import { EVERY_TYPE, EVERY_TYPE_BUT_EPISODES, ORDERS, STATUS_CHOICES, cleared, filterOf, honours, orderOf, pathOf, placeOf, queryOf, standing } from "../../domain/brain-place";
import { subjectTypeName, typeName } from "../../domain/brain-names";
import { ENGLISH, useReading } from "../../domain/brain-reading";
import { isBase } from "../../domain/brain-review";
import { useBrainStream } from "../../domain/brain-stream";

import type { Brain, Layer, Memory, Subject, Tally, TypeCount } from "../../domain/brain-api";
import type { Heading, HeadingChip } from "../../domain/brain-heading";
import type { Choice, Order, Place, StatusChoice, View } from "../../domain/brain-place";
import type { BarAction } from "bmo-ui/app/domain/tabs";
import type { DropdownMenuItem, SplitterItem } from "@nuxt/ui";

// The screen is one screen for every place inside it, so a memory opened or a filter changed reads again
// what it needs and never builds the screen anew.
definePageMeta({ key: () => "brain" });

const SIDES: SplitterItem[] = [
  { slot: "index", minSize: 12, defaultSize: 20, collapsible: true, collapsedSize: 0 },
  { slot: "view", minSize: 30, defaultSize: 48 },
  { slot: "row", minSize: 20, defaultSize: 32, collapsible: true, collapsedSize: 0 }
];

// With no memory open the view takes the row pane's room: no pane stands empty.
const TWO_SIDES: SplitterItem[] = [
  { slot: "index", minSize: 12, defaultSize: 22, collapsible: true, collapsedSize: 0 },
  { slot: "view", minSize: 40, defaultSize: 78 }
];

const PAGE = 50;

const route = useRoute();
const router = useRouter();
const { t, te } = useI18n();
// Whether the console this screen stands in offers its title bar, under the key the layer names for it. Only its
// presence is read: the screen lays nothing into it.
const bar = inject<object | null>("bmo-console-title-bar", null);
const island = useIsland(t);
const narrow = useNarrow();
// Whether the toolbar gathers, by the room it has rather than the window's: then the words join the other filters,
// so the actions at its end stay in sight.
const toolbar = useTemplateRef<{ $el: HTMLElement }>("toolbar");
const barGathers = useBarGathers(() => toolbar.value?.$el);
const gathered = computed(() => narrow.value || barGathers.value);
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
const layerAddOpen = ref(false);
const create = useTemplateRef<{ element: () => HTMLElement | null }>("create");
// Bumped whenever the brain says something changed, so the parts that read on their own read again.
const changes = ref(0);

// The screen's actions on the screen as a whole, at the toolbar's end: the reading language where a translator is
// named.
const screenActions = computed<BarAction[]>(() =>
  reader.offered
    ? [
        {
          id: "reading",
          label: t("brain.action.reading"),
          icon: "i-material-symbols-translate",
          pressed: translating.value,
          onSelect: () => reader.choose(translating.value ? ENGLISH : reader.home)
        }
      ]
    : []
);

// What the create button makes: a memory, by teaching, or a layer.
const createItems = computed<DropdownMenuItem[]>(() => [
  {
    label: t("brain.action.teach"),
    icon: "i-material-symbols-school-outline",
    onSelect: () => {
      teachOpen.value = true;
    }
  },
  {
    label: t("brain.action.layer_add"),
    icon: "i-material-symbols-layers-outline",
    onSelect: () => {
      layerAddOpen.value = true;
    }
  }
]);

// The create menu's content stands against the button itself, wherever it stands at each placing: in the
// toolbar, where the menu's trigger is the button's cell, which takes no focus, or floating on a phone, where
// the trigger has no box of its own in the row. The focus the menu held goes back to the button as it closes,
// and a focus the reader already moved elsewhere stays there. It is one object from the first render, as the
// menu reads only the settings it was first handed.
function createButton(): HTMLElement | null {
  return create.value?.element() ?? null;
}

const createMenu = {
  align: "end" as const,
  reference: {
    getBoundingClientRect: (): DOMRect => createButton()?.getBoundingClientRect() ?? new DOMRect(),
    get contextElement(): HTMLElement | undefined {
      return createButton() ?? undefined;
    }
  },
  onCloseAutoFocus: (event: Event): void => {
    event.preventDefault();

    const focused = document.activeElement;
    const menu = event.target;

    if (focused === null || focused === document.body || (menu instanceof Node && menu.contains(focused))) createButton()?.focus();
  }
};

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

// The brain's select, standing in the toolbar or, on a phone, at the head of the index.
const brainSelect = computed(() => ({
  modelValue: place.value.brainId ?? undefined,
  items: brainItems.value,
  valueKey: "value",
  placeholder: t("brain.label.brain"),
  "aria-label": t("brain.label.brain"),
  "onUpdate:modelValue": (brainId: unknown) => chooseBrain(Number(brainId))
}));

// The words the view is narrowed by, standing in the toolbar or, on a phone, among the filters.
const wordsInput = computed(() => ({
  modelValue: place.value.q,
  "aria-label": t("brain.label.words"),
  disabled: !honours(place.value.view, "q"),
  onChange: (event: Event) => go({ q: (event.target as HTMLInputElement).value, memory: null })
}));

// Whether the brain chosen is the base, whose candidates the review view lists.
const baseChosen = computed(() => isBase((brainList.value ?? []).find((brain) => brain.id === place.value.brainId)));

// The views, the review among them only while the base is chosen.
const viewItems = computed(() =>
  (
    [
      ["list", "i-material-symbols-list"],
      ["map", "i-material-symbols-hub-outline"],
      ["bubbles", "i-material-symbols-bubble-chart-outline"],
      ["timeline", "i-material-symbols-timeline"],
      ["book", "i-material-symbols-menu-book-outline"],
      ["review", "i-material-symbols-fact-check-outline"]
    ] as const
  )
    .filter(([value]) => value !== "review" || baseChosen.value)
    .map(([value, icon]) => ({ value: value as View, label: t(`brain.view.${value}`), icon }))
);

function countOf(tallied: Tally[], groups: string[]): number {
  return tallied.filter((tally) => tally.group !== null && groups.includes(tally.group)).reduce((sum, tally) => sum + tally.count, 0);
}

const statusItems = computed(() =>
  STATUS_CHOICES.map((choice) => {
    const all = statusCounts.value.reduce((sum, tally) => sum + tally.count, 0);
    const count = choice === "all" ? all : choice === "live" ? countOf(statusCounts.value, ["active", "pending"]) : countOf(statusCounts.value, [choice]);

    return { value: choice, label: `${t(`brain.status_choice.${choice}`)} · ${count.toLocaleString()}` };
  })
);

const typeItems = computed(() => {
  const all = typeCounts.value.reduce((sum, tally) => sum + tally.count, 0);
  const episodes = countOf(typeCounts.value, ["episode"]);
  const named = typeList.value.map((type) => ({
    value: type.type,
    label: `${typeName(t, te, type.type)} · ${countOf(typeCounts.value, [type.type]).toLocaleString()}`
  }));

  return [
    { value: EVERY_TYPE_BUT_EPISODES, label: `${t("brain.type_choice.but_episodes")} · ${(all - episodes).toLocaleString()}` },
    { value: EVERY_TYPE, label: `${t("brain.type_choice.every")} · ${all.toLocaleString()}` },
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

  return subject === undefined ? `#${id}` : (subject.name ?? `${subjectTypeName(t, te, subject.type)} ${subject.key}`);
}

// Why a view does not honour a choice, in one line, or nothing where it does.
function unhonoured(choice: Choice): string {
  if (honours(place.value.view, choice)) return "";

  if (place.value.view === "review") return t("brain.unhonoured.review");

  return place.value.view === "timeline" ? t("brain.unhonoured.timeline") : t("brain.unhonoured.order");
}

// The value a standing choice reads as, in words.
function choiceValue(choice: Choice): string {
  const at = place.value;

  switch (choice) {
    case "layer":
      return at.layer === null ? "" : layerName(at.layer);
    case "subject":
      return at.subject === null ? "" : subjectName(at.subject);
    case "type":
      return at.type === EVERY_TYPE ? t("brain.type_choice.every") : typeName(t, te, at.type);
    case "status":
      return t(`brain.status_choice.${at.status}`);
    case "order":
      return t(`brain.order.${at.order.replace("-", "desc_")}`);
    default:
      return at[choice];
  }
}

// The heading every view stands under: the brain and the view, each standing choice, and why any is not honoured.
const heading = computed<Heading>(() => {
  const brain = (brainList.value ?? []).find((found) => found.id === place.value.brainId);
  const view = t(`brain.view.${place.value.view}`);
  const chips: HeadingChip[] = standing(place.value).map((choice) => ({
    choice,
    what: t(`brain.heading.${choice}`),
    value: choiceValue(choice),
    honoured: honours(place.value.view, choice),
    clear: () => go({ ...cleared(), ...keep(choice), memory: place.value.memory })
  }));
  const notes = [...new Set(chips.filter((chip) => !chip.honoured).map((chip) => unhonoured(chip.choice as Choice)))];

  return {
    title: t("brain.heading.brain_view", { brain: brain?.name || brain?.agentKey || "", view }),
    chips,
    notes,
    clearAll: () => go({ ...cleared(), memory: place.value.memory })
  };
});

// Every standing choice kept but one.
function keep(without: Choice): Partial<Place> {
  const kept: Partial<Place> = {};

  for (const choice of standing(place.value)) {
    if (choice !== without) (kept as Record<string, unknown>)[choice] = place.value[choice];
  }

  return kept;
}

// What the memory list holds under every choice, read off the status counts.
const listCount = computed(() => {
  const choice = place.value.status;

  if (choice === "all") return statusCounts.value.reduce((sum, tally) => sum + tally.count, 0);

  return countOf(statusCounts.value, choice === "live" ? ["active", "pending"] : [choice]);
});

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

const subjectNames = computed(() => new Map(subjectList.value.map((subject) => [subject.id, subject.name ?? `${subjectTypeName(t, te, subject.type)} ${subject.key}`])));

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
    case "review":
      return baseChosen.value ? BrainReview : BrainList;
    default:
      return BrainList;
  }
});

const viewProps = computed(() => {
  const brainId = place.value.brainId ?? 0;
  const common = { brainId, heading: heading.value, changes: changes.value };

  switch (place.value.view === "review" && !baseChosen.value ? "list" : place.value.view) {
    case "review":
      return {
        ...common,
        brains: brainList.value ?? [],
        onSource: (source: { brainId: number; memoryId: number }) =>
          void router.push({ path: pathOf({ ...place.value, ...cleared(), brainId: source.brainId, view: "list" }), query: { memory: String(source.memoryId) } })
      };
    case "map":
      return {
        ...common,
        subjectId: place.value.subject,
        filter: filterOf(place.value),
        subjects: subjectList.value,
        layers: layerList.value,
        onOpen: openMemory,
        onCentre: (subject: number) => go({ subject }),
        onNarrow: (subject: number) => go({ subject, view: "list" })
      };
    case "bubbles":
      return {
        ...common,
        by: place.value.by,
        filter: filterOf(place.value),
        layers: layerList.value,
        onBy: (by: "type" | "layer") => go({ by }),
        onNarrow: (patch: Partial<Place>) => go({ ...patch, view: "list" })
      };
    case "timeline":
      return {
        ...common,
        memoryId: place.value.memory,
        onOpen: openMemory
      };
    case "book":
      return {
        ...common,
        layers: layerList.value,
        layer: place.value.layer,
        filter: filterOf(place.value),
        onOpen: openMemory,
        onChanged: reread
      };
    default:
      return {
        ...common,
        count: listCount.value,
        memories: memories.value,
        more: more.value,
        loadingMore: loadingMore.value,
        failure: listFailure.value,
        layers: layerList.value,
        subjectNames: subjectNames.value,
        openId: place.value.memory,
        narrow: narrow.value,
        subjectId: place.value.subject,
        onOpen: openMemory,
        onMore: readMore
      };
  }
});
</script>
