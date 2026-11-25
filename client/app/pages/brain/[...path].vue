<template>
  <!-- The brain screen (docs/design.md, 「The screen」): one brain at a time. Where there is room the index,
       the view and the memory open stand side by side behind splitters, each scrolling on its own; on a
       phone the view is the body, the index rises from the bottom edge and a memory stands over the view.
       The brain, the view, the filters and the memory open are the address. An org brain is read here and never
       written: it is written on the console's organisation screen. -->
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
                      :find="findTypes"
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
          <UDropdownMenu :items="createItems" :disabled="!creatable" :content="createMenu">
            <CreateButton
              ref="create"
              :class="{ 'ms-auto': screenActions.length === 0 }"
              :label="orgChosen ? t('brain.hint.org_read_only') : t('brain.action.create')"
              :disabled="!creatable"
            />
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
              <BrainIndex v-bind="indexProps" v-model:tab="indexTab" v-model:words="indexWords" @choose="indexOpen = false; choose($event)" @changed="reread" />
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
              <BrainIndex v-bind="indexProps" v-model:tab="indexTab" v-model:words="indexWords" @choose="choose" @changed="reread" />
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
          v-if="place.brainId !== null && !orgChosen"
          v-model:open="teachOpen"
          :brain-id="place.brainId"
          :layers="layerList"
          :narrow="narrow"
          @taught="taught"
        />
        <BrainLayerAdd v-if="place.brainId !== null && !orgChosen" v-model:open="layerAddOpen" :brain-id="place.brainId" @added="reread" />
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
import BrainRow from "../../components/BrainRow.vue";
import BrainTeach from "../../components/BrainTeach.vue";
import BrainTimeline from "../../components/BrainTimeline.vue";
import * as api from "../../domain/brain-api";
import { EVERY_TYPE, EVERY_TYPE_BUT_EPISODES, ORDERS, STATUS_CHOICES, cleared, filterOf, honours, orderOf, pathOf, placeOf, queryOf, standing } from "../../domain/brain-place";
import { subjectTypeName, typeName, typeSearchNames } from "../../domain/brain-names";
import { ENGLISH, useReading } from "../../domain/brain-reading";
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
// The first page of an index list, and the most the index reads at once.
const INDEX_PAGE = 50;

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
// The index's three lists, each read a page at a time: whether a list stands loaded, whether a page is on its way,
// whether more stands past its last page, the cursor the last page ended on, and the words it was read under.
type IndexTab = "layers" | "subjects" | "types";
const indexTab = ref<IndexTab>("layers");
const indexWords = ref("");
// The names the reader's language gives the types, sent with the words so a type is found by the name it shows
// (docs/design.md, 「The screen」, "Names in the reader's language").
const typeNames = computed(() => typeSearchNames(t, te));
const layerMore = ref(false);
const subjectMore = ref(false);
// The type filter's own first page, read at the brain for the choices it opens with, and the chosen type among them.
const choiceTypes = ref<TypeCount[]>([]);
const typeMore = ref(false);
const layerBusy = ref(false);
const subjectBusy = ref(false);
const typeBusy = ref(false);
const layerLoaded = ref(false);
const subjectLoaded = ref(false);
const typeLoaded = ref(false);
const layerCursor = ref<number | null>(null);
const subjectCursor = ref<number | null>(null);
const typeCursor = ref<string | null>(null);
// The words each list was read under, so a tab opened after the search changed reads it afresh.
const wordsOf: Record<IndexTab, string> = { layers: "", subjects: "", types: "" };
// Bumped when a first page lands or a chosen member stands in a list, so the index brings the chosen row into sight
// then, and never as a next page lands under a reader scrolled down.
const reveal = ref(0);
// The names of the layers and the subjects the memories on screen hold, read for those the index's pages do not carry.
const namedLayers = ref(new Map<number, string>());
const namedSubjects = ref(new Map<number, string>());
const memories = ref<Memory[] | null>(null);
const more = ref(false);
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

// Whether the brain chosen is an org brain, the base among them, which the screen reads and never writes: it is
// written on the console's organisation screen (docs/design.md, 「The screen」).
const orgChosen = computed(() => (brainList.value ?? []).find((brain) => brain.id === place.value.brainId)?.kind === "org");

// Whether the create button makes anything: a brain is chosen and it is an agent's.
const creatable = computed(() => place.value.brainId !== null && !orgChosen.value);

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

    return { value: choice, label: `${t(`brain.status_choice.${choice}`)} · ${count.toLocaleString()}` };
  })
);

const typeItems = computed(() => {
  const all = statusCounts.value.reduce((sum, tally) => sum + tally.count, 0);
  const episodes = countOf(typeCounts.value, ["episode"]);
  const named = choiceTypes.value.map((type) => ({
    value: type.type,
    label: `${typeName(t, te, type.type)} · ${countOf(typeCounts.value, [type.type]).toLocaleString()}`
  }));

  return [
    { value: EVERY_TYPE_BUT_EPISODES, label: `${t("brain.type_choice.but_episodes")} · ${(all - episodes).toLocaleString()}` },
    { value: EVERY_TYPE, label: `${t("brain.type_choice.every")} · ${all.toLocaleString()}` },
    ...named
  ];
});

// The type filter's choices found at the brain for the words typed, each with the count of the memories the other
// filters narrow it to: the road the picker searches by, past the first page the list stands on.
async function findTypes(words: string): Promise<{ value: string; label: string; description?: string }[]> {
  const brainId = place.value.brainId;
  const asked = words.trim();

  if (brainId === null || asked === "") return [];

  // A reserved type is shown by its name in the reader's language, so the choices read already are searched by that
  // name; the road adds the types the brain finds by their own word, which is the word a type of the writer's has.
  const countOf = (kind: string): string =>
    (typeCounts.value.find((tally) => tally.group === kind)?.count ?? 0).toLocaleString();
  const choice = (type: TypeCount, count: string) => {
    const name = typeName(t, te, type.type);

    return { value: type.type, label: `${name} · ${count}`, ...(name === type.type ? {} : { description: type.type }) };
  };
  const lower = asked.toLocaleLowerCase();
  const local = choiceTypes.value
    .filter((type) => typeName(t, te, type.type).toLocaleLowerCase().includes(lower))
    .map((type) => choice(type, countOf(type.type)));

  try {
    const page = await api.typePage(brainId, { q: asked, names: typeNames.value, limit: INDEX_PAGE });
    const kinds = page.items.map((type) => type.type);
    const counted =
      kinds.length === 0 ? [] : await api.tallies(brainId, "type", filterOf(place.value, "type"), kinds.join(","));
    const found = new Set(local.map((one) => one.value));

    return [
      ...local,
      ...page.items
        .filter((type) => !found.has(type.type))
        .map((type) => choice(type, (counted.find((tally) => tally.group === type.type)?.count ?? 0).toLocaleString()))
    ];
  } catch {
    return local;
  }
}

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

// The heading every view stands under: the brain and the view, each standing choice, why any is not honoured, and
// where an org brain is written.
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
  const notes = [
    ...(orgChosen.value ? [t("brain.hint.org_read_only")] : []),
    ...new Set(chips.filter((chip) => !chip.honoured).map((chip) => unhonoured(chip.choice as Choice)))
  ];

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

// The index's lists are a person's view of a brain, read fifty at a time and the next fifty as the index's end
// comes into view (bmo-ui's NextPageFoot). Each page's groups are counted alone, and a chosen member standing past
// the pages read is read by its id so its name and its row stand.

const TABS: IndexTab[] = ["layers", "subjects", "types"];

function shownOf(tab: IndexTab): { length: number } {
  return tab === "layers" ? layerList.value : tab === "subjects" ? subjectList.value : typeList.value;
}

function busyOf(tab: IndexTab) {
  return tab === "layers" ? layerBusy : tab === "subjects" ? subjectBusy : typeBusy;
}

function loadedOf(tab: IndexTab) {
  return tab === "layers" ? layerLoaded : tab === "subjects" ? subjectLoaded : typeLoaded;
}

// A count's groups merged into those already counted, the later read winning for a group.
function mergeCounts(kept: Tally[], counted: Tally[]): Tally[] {
  const byGroup = new Map(kept.map((tally) => [tally.group, tally]));

  for (const tally of counted) byGroup.set(tally.group, tally);

  return [...byGroup.values()];
}

function mergeById<T extends { id: number }>(kept: T[], added: T[]): T[] {
  const byId = new Map(kept.map((one) => [one.id, one]));

  for (const one of added) byId.set(one.id, one);

  return [...byId.values()];
}

function mergeTypes(kept: TypeCount[], added: TypeCount[]): TypeCount[] {
  const byKind = new Map(kept.map((one) => [one.type, one]));

  for (const one of added) byKind.set(one.type, one);

  return [...byKind.values()].sort(byType);
}

function byPosition(first: Layer, second: Layer): number {
  return first.position - second.position || first.id - second.id;
}

function byId(first: Subject, second: Subject): number {
  return first.id - second.id;
}

// The reserved types first, in the order the route gives them, then every other by its name.
function byType(first: TypeCount, second: TypeCount): number {
  if (first.reserved !== second.reserved) return first.reserved ? -1 : 1;

  return first.reserved ? 0 : first.type.localeCompare(second.type);
}

// The types in use: a reserved type with no live row stands in the route's list but not the screen's, but for the
// type the person narrowed the view to, which stands so the choice keeps its row.
function usedTypes(types: TypeCount[]): TypeCount[] {
  return types.filter((type) => type.count > 0 || type.type === place.value.type);
}

// The live rows of the layers and the subjects a page holds, and the unfiled's on its own.
async function readLayerCounts(brainId: number, shown: Layer[]): Promise<void> {
  const counted =
    shown.length === 0
      ? []
      : await api.tallies(brainId, "layer", { status: "active,pending" }, shown.map((layer) => String(layer.id)).join(","));
  const unfiled = await api.tallies(brainId, "layer", { status: "active,pending", unfiled: true });

  layerCounts.value = mergeCounts(mergeCounts(layerCounts.value, counted), unfiled);
}

async function readSubjectCounts(brainId: number, shown: Subject[]): Promise<void> {
  if (shown.length === 0) return;

  const counted = await api.tallies(
    brainId,
    "subject",
    { status: "active,pending" },
    shown.map((subject) => String(subject.id)).join(",")
  );

  subjectCounts.value = mergeCounts(subjectCounts.value, counted);
}

const chosenLayer = (): number | null => (typeof place.value.layer === "number" ? place.value.layer : null);

// A chosen member standing past the pages read is read by its id, so its name stands and its row is in sight.
async function ensureLayer(brainId: number, id: number | null): Promise<void> {
  if (id === null || layerList.value.some((layer) => layer.id === id)) return;

  try {
    const found = await api.layer(brainId, id);

    layerList.value = mergeById(layerList.value, [found]).sort(byPosition);
    reveal.value++;
    await readLayerCounts(brainId, [found]);
  } catch {
    // A layer the brain no longer holds stands by its number in the heading.
  }
}

async function ensureSubject(brainId: number, id: number | null): Promise<void> {
  if (id === null || subjectList.value.some((subject) => subject.id === id)) return;

  try {
    const page = await api.subjectPage(brainId, { ids: String(id) });
    const found = page.items.find((subject) => subject.id === id);

    if (found === undefined) return;

    subjectList.value = mergeById(subjectList.value, [found]).sort(byId);
    reveal.value++;
    await readSubjectCounts(brainId, [found]);
  } catch {
    // A subject the brain no longer holds stands by its number in the heading.
  }
}

function concrete(type: string): boolean {
  return type !== EVERY_TYPE && type !== EVERY_TYPE_BUT_EPISODES && !type.startsWith("*");
}

async function ensureType(brainId: number, kind: string): Promise<void> {
  if (!concrete(kind) || typeList.value.some((type) => type.type === kind)) return;

  try {
    const page = await api.typePage(brainId, { q: kind, limit: 500 });
    const found = page.items.find((type) => type.type === kind);

    if (found === undefined) return;

    typeList.value = mergeTypes(typeList.value, [found]);
    reveal.value++;
  } catch {
    // A type the brain no longer holds stands by its word in the heading.
  }
}

// The chosen type stands among the filter's choices even where it lies past their first page.
async function ensureChoiceType(brainId: number, kind: string): Promise<void> {
  if (!concrete(kind) || choiceTypes.value.some((type) => type.type === kind)) return;

  try {
    const page = await api.typePage(brainId, { q: kind, limit: 500 });
    const found = page.items.find((type) => type.type === kind);

    if (found === undefined) return;

    choiceTypes.value = mergeTypes(choiceTypes.value, [found]);
    await readCounts(brainId);
  } catch {
    // The type's count stands at nothing where the brain no longer holds it.
  }
}

// The type filter's first page, read at the brain for the choices it opens with.
let filterEpoch = 0;
async function readFilterTypes(brainId: number): Promise<void> {
  const epoch = ++filterEpoch;

  try {
    const page = await api.typePage(brainId, { limit: INDEX_PAGE });

    if (epoch !== filterEpoch) return;

    choiceTypes.value = usedTypes(page.items);
    await readCounts(brainId);
    await ensureChoiceType(brainId, place.value.type);
  } catch {
    // The filter's list stands empty; its search still reaches every type.
  }
}

// Each tab's reads are taken in turn: a read a newer one overtook leaves the list as the newer one found it, so a
// brain changed or words typed while a page is on its way never has the older answer drawn over it.
const tokens: Record<IndexTab, number> = { layers: 0, subjects: 0, types: 0 };

function claims(tab: IndexTab): number {
  return ++tokens[tab];
}

function current(tab: IndexTab, token: number): boolean {
  return tokens[tab] === token;
}

function release(tab: IndexTab, token: number): void {
  if (current(tab, token)) busyOf(tab).value = false;
}

// A list's first page, or as many members as were read already where the list is read again.
async function readFirst(brainId: number, tab: IndexTab, wanted = INDEX_PAGE): Promise<void> {
  const token = claims(tab);

  busyOf(tab).value = true;

  try {
    if (tab === "layers") {
      const page = await api.layerPage(brainId, { q: indexWords.value, limit: Math.min(wanted, 500) });

      if (!current(tab, token)) return;

      layerList.value = page.items;
      layerMore.value = page.more;
      layerCursor.value = page.items.at(-1)?.id ?? null;
      layerLoaded.value = true;
      wordsOf.layers = indexWords.value;
      layerCounts.value = [];
      await readLayerCounts(brainId, page.items);
      if (!current(tab, token)) return;
      await ensureLayer(brainId, chosenLayer());
      reveal.value++;
    } else if (tab === "subjects") {
      const page = await api.subjectPage(brainId, { q: indexWords.value, limit: Math.min(wanted, 500) });

      if (!current(tab, token)) return;

      subjectList.value = page.items;
      subjectMore.value = page.more;
      subjectCursor.value = page.items.at(-1)?.id ?? null;
      subjectLoaded.value = true;
      wordsOf.subjects = indexWords.value;
      subjectCounts.value = [];
      await readSubjectCounts(brainId, page.items);
      if (!current(tab, token)) return;
      await ensureSubject(brainId, place.value.subject);
      reveal.value++;
    } else {
      const page = await api.typePage(brainId, { q: indexWords.value, names: indexWords.value.trim() === "" ? undefined : typeNames.value, limit: Math.min(wanted, 500) });

      if (!current(tab, token)) return;

      typeList.value = usedTypes(page.items);
      typeMore.value = page.more;
      typeCursor.value = page.items.at(-1)?.type ?? null;
      typeLoaded.value = true;
      wordsOf.types = indexWords.value;
      if (!current(tab, token)) return;
      // The first page under no words is the type filter's own list, counted under the filters it opens with.
      if (indexWords.value === "") {
        choiceTypes.value = usedTypes(page.items);
        await readCounts(brainId);
        if (!current(tab, token)) return;
        await ensureChoiceType(brainId, place.value.type);
      }
      await ensureType(brainId, place.value.type);
      reveal.value++;
    }
  } catch (error) {
    if (current(tab, token)) {
      loadedOf(tab).value = false;
      island.fail(error, t("brain.error.read"));
    }
  } finally {
    release(tab, token);
  }
}

// One page more, after the member the last page ended on; a failure is thrown to the foot, which keeps what was read
// and stands its retry in place of the page.
async function readNext(tab: IndexTab): Promise<void> {
  const brainId = place.value.brainId;

  if (brainId === null) return;

  const token = claims(tab);

  busyOf(tab).value = true;

  try {
    if (tab === "layers") {
      const page = await api.layerPage(brainId, { q: indexWords.value, after: layerCursor.value, limit: INDEX_PAGE });

      if (!current(tab, token)) return;

      layerList.value = mergeById(layerList.value, page.items).sort(byPosition);
      layerMore.value = page.more;
      layerCursor.value = page.items.at(-1)?.id ?? layerCursor.value;
      await readLayerCounts(brainId, page.items);
    } else if (tab === "subjects") {
      const page = await api.subjectPage(brainId, { q: indexWords.value, after: subjectCursor.value, limit: INDEX_PAGE });

      if (!current(tab, token)) return;

      subjectList.value = mergeById(subjectList.value, page.items).sort(byId);
      subjectMore.value = page.more;
      subjectCursor.value = page.items.at(-1)?.id ?? subjectCursor.value;
      await readSubjectCounts(brainId, page.items);
    } else {
      const page = await api.typePage(brainId, { q: indexWords.value, names: indexWords.value.trim() === "" ? undefined : typeNames.value, after: typeCursor.value, limit: INDEX_PAGE });

      if (!current(tab, token)) return;

      typeList.value = usedTypes(mergeTypes(typeList.value, page.items));
      typeMore.value = page.more;
      typeCursor.value = page.items.at(-1)?.type ?? typeCursor.value;
    }
  } finally {
    release(tab, token);
  }
}

// A list taken back to nothing, so the tab that opens next reads it afresh; a read on its way is overtaken.
function clearTab(tab: IndexTab): void {
  tokens[tab]++;
  busyOf(tab).value = false;

  if (tab === "layers") {
    layerList.value = [];
    layerMore.value = false;
    layerLoaded.value = false;
    layerCursor.value = null;
    layerCounts.value = [];
  } else if (tab === "subjects") {
    subjectList.value = [];
    subjectMore.value = false;
    subjectLoaded.value = false;
    subjectCursor.value = null;
    subjectCounts.value = [];
  } else {
    typeList.value = [];
    typeMore.value = false;
    typeLoaded.value = false;
    typeCursor.value = null;
  }
}

// The open tab reads its first page, under the words standing now; one not yet opened reads when it is.
function ensureTab(brainId: number, tab: IndexTab): void {
  if (loadedOf(tab).value && wordsOf[tab] === indexWords.value) return;

  clearTab(tab);
  void readFirst(brainId, tab);
}

// The counts beside the status and type choices, each under every filter but its own. The type choices' counts are
// the filter's types' groups and the episodes' own, read for the groups the picker shows alone.
let countsToken = 0;
async function readCounts(brainId: number): Promise<void> {
  const token = ++countsToken;
  const typeFilter = filterOf(place.value, "type");
  const kinds = choiceTypes.value.map((type) => type.type).join(",");
  const episodes = api.tallies(brainId, "type", typeFilter, "episode");
  const loaded = kinds === "" ? Promise.resolve<Tally[]>([]) : api.tallies(brainId, "type", typeFilter, kinds);

  try {
    const [byStatus, byEpisode, byType] = await Promise.all([
      api.tallies(brainId, "status", filterOf(place.value, "status")),
      episodes,
      loaded
    ]);

    if (token !== countsToken) return;

    statusCounts.value = byStatus;
    typeCounts.value = mergeCounts(byEpisode, byType);
  } catch (error) {
    if (token === countsToken) island.fail(error, t("brain.error.read"));
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
    void nameLayers(brainId, page.items);
    void nameSubjects(brainId, page.items);
  } catch (error) {
    listFailure.value = t("brain.error.read");
    island.fail(error, t("brain.error.read"));
  }
}

// One page more, after the last memory held, read as the list's foot comes into view; a failure is thrown to the foot,
// which keeps the memories already read and offers the retry in the failed page's place.
async function readMore(): Promise<void> {
  const brainId = place.value.brainId;
  const last = memories.value?.[memories.value.length - 1];

  if (brainId === null || last === undefined) return;

  const page = await api.memoryPage(brainId, filterOf(place.value), orderOf(place.value), last.id, PAGE);

  memories.value = [...(memories.value ?? []), ...page.items];
  more.value = page.more;
  void nameLayers(brainId, page.items);
  void nameSubjects(brainId, page.items);
}

// The names of the layers named, read for those the index's pages do not carry.
async function nameLayerIds(brainId: number, ids: number[]): Promise<void> {
  const known = new Set([...layerList.value.map((layer) => layer.id), ...namedLayers.value.keys()]);
  const wanted = [...new Set(ids)].filter((id) => !known.has(id));

  if (wanted.length === 0) return;

  try {
    const found = await Promise.all(wanted.slice(0, 50).map((id) => api.layer(brainId, id).catch(() => null)));
    const names = new Map(namedLayers.value);

    for (const layer of found) if (layer !== null) names.set(layer.id, layer.name);
    namedLayers.value = names;
  } catch {
    // A layer the brain no longer holds is shown by its number.
  }
}

// The names of the layers a page of memories holds.
async function nameLayers(brainId: number, page: Memory[]): Promise<void> {
  await nameLayerIds(
    brainId,
    page.map((memory) => memory.layerId).filter((id): id is number => id !== null && id !== undefined)
  );
}

// The names of the subjects a page of memories holds, read for those the index's pages do not carry.
async function nameSubjects(brainId: number, page: Memory[]): Promise<void> {
  const known = new Set([...subjectList.value.map((subject) => subject.id), ...namedSubjects.value.keys()]);
  const wanted = [...new Set(page.flatMap((memory) => memory.subjects.map((subject) => subject.id)))].filter(
    (id) => !known.has(id)
  );

  if (wanted.length === 0) return;

  try {
    const found = await api.subjectPage(brainId, { ids: wanted.join(","), limit: Math.min(wanted.length, 500) });
    const names = new Map(namedSubjects.value);

    for (const subject of found.items) names.set(subject.id, subject.name ?? `${subjectTypeName(t, te, subject.type)} ${subject.key}`);
    namedSubjects.value = names;
  } catch {
    // A subject the brain no longer holds is shown by its key.
  }
}

// What the brain said changed, read again where it stands; the open list keeps the pages it had read.
function reread(): void {
  const brainId = place.value.brainId;

  changes.value++;

  if (brainId === null) return;

  const tab = indexTab.value;
  const depth = Math.min(500, Math.max(INDEX_PAGE, shownOf(tab).length));

  for (const other of TABS) if (other !== tab) clearTab(other);
  void readFirst(brainId, tab, depth);
  if (!(tab === "types" && indexWords.value === "")) void readFilterTypes(brainId);
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
    namedLayers.value = new Map();
    namedSubjects.value = new Map();
    choiceTypes.value = [];
    for (const tab of TABS) clearTab(tab);
    // Each list opens with its first page, bounded; the rest comes as the index is scrolled.
    void readFirst(brainId, "layers");
    void readFirst(brainId, "subjects");
    void readFirst(brainId, "types");
    if (indexWords.value !== "") void readFilterTypes(brainId);
  },
  { immediate: true }
);

// The open tab reads its first page when it is opened, under the words standing then.
watch(indexTab, (tab) => {
  const brainId = place.value.brainId;

  if (brainId !== null) ensureTab(brainId, tab);
});

// The words narrow the three lists at the brain; the open tab reads afresh once typing stands still.
let wordsTimer: ReturnType<typeof setTimeout> | undefined;
watch(indexWords, () => {
  const brainId = place.value.brainId;

  if (brainId === null) return;
  if (wordsTimer !== undefined) clearTimeout(wordsTimer);
  wordsTimer = setTimeout(() => {
    // The brain may have changed under the words; the lists of the one standing read now.
    if (place.value.brainId !== brainId) return;
    // The open list reads under the new words; another tab reads when it is opened.
    clearTab(indexTab.value);
    void readFirst(brainId, indexTab.value);
  }, 250);
});

// A chosen layer, subject or type standing past the pages read is read by its id.
watch(
  () => [place.value.layer, place.value.subject, place.value.type] as const,
  () => {
    const brainId = place.value.brainId;

    if (brainId === null) return;

    void ensureLayer(brainId, chosenLayer());
    void ensureSubject(brainId, place.value.subject);
    void ensureType(brainId, place.value.type);
    void ensureChoiceType(brainId, place.value.type);
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

const layerNames = computed(() => {
  const names = new Map<number, string>();

  for (const layer of layerList.value) names.set(layer.id, layer.name);
  for (const [id, name] of namedLayers.value) names.set(id, name);

  return names;
});

const subjectNames = computed(() => {
  const names = new Map<number, string>();

  for (const subject of subjectList.value) names.set(subject.id, subject.name ?? `${subjectTypeName(t, te, subject.type)} ${subject.key}`);
  for (const [id, name] of namedSubjects.value) names.set(id, name);

  return names;
});

const indexProps = computed(() => ({
  brainId: place.value.brainId,
  layers: layerList.value,
  layerCounts: layerCounts.value,
  subjects: subjectList.value,
  subjectCounts: subjectCounts.value,
  types: typeList.value,
  place: place.value,
  readOnly: orgChosen.value,
  more: { layers: layerMore.value, subjects: subjectMore.value, types: typeMore.value },
  busy: { layers: layerBusy.value, subjects: subjectBusy.value, types: typeBusy.value },
  next: readNext,
  reveal: reveal.value
}));

const rowProps = computed(() => ({
  brainId: place.value.brainId ?? 0,
  memoryId: place.value.memory ?? 0,
  layers: layerList.value,
  subjectNames: subjectNames.value,
  changes: changes.value,
  readOnly: orgChosen.value
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
  const common = { brainId, heading: heading.value, changes: changes.value };

  switch (place.value.view) {
    case "map":
      return {
        ...common,
        subjectId: place.value.subject,
        filter: filterOf(place.value),
        subjects: subjectList.value,
        layerNames: layerNames.value,
        onNames: (page: Memory[]) => void nameLayers(brainId, page),
        onOpen: openMemory,
        onCentre: (subject: number) => go({ subject }),
        onNarrow: (subject: number) => go({ subject, view: "list" })
      };
    case "bubbles":
      return {
        ...common,
        by: place.value.by,
        filter: filterOf(place.value),
        layerNames: layerNames.value,
        onNames: (ids: number[]) => void nameLayerIds(brainId, ids),
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
        readOnly: orgChosen.value,
        onOpen: openMemory,
        onChanged: reread
      };
    default:
      return {
        ...common,
        count: listCount.value,
        memories: memories.value,
        more: more.value,
        next: readMore,
        failure: listFailure.value,
        layerNames: layerNames.value,
        subjectNames: subjectNames.value,
        openId: place.value.memory,
        narrow: narrow.value,
        subjectId: place.value.subject,
        onOpen: openMemory
      };
  }
});
</script>
