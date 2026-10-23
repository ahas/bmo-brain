<template>
  <!-- The layer index read as a book (docs/design.md, 「The screen」): the layers in position order, each
       layer's pieces in theirs, moved up and down as new versions. A layer holding persona pieces shows its
       rendering, the very bytes the brain hands a system prompt; a layer is exported as a brain file. -->
  <div class="flex min-h-0 flex-1 flex-col">
    <ToolBar :label="t('brain.view.book')">
      <div>
        <USelect :model-value="current?.id" :items="layerItems" :placeholder="t('brain.field.layer')" variant="ghost" class="min-w-40" @update:model-value="emit('layer', Number($event))" />
      </div>
      <TabSwitch v-if="hasPersona" v-model="reading" :items="readings" :label="t('brain.label.reading')" />
      <div class="ms-auto">
        <UButton v-if="current" icon="i-material-symbols-download" color="neutral" variant="ghost" :aria-label="t('brain.action.export')" :to="exportHref" external download />
      </div>
    </ToolBar>

    <p v-if="current === undefined" class="p-6 text-center text-sm text-muted">{{ t("brain.status.no_layers") }}</p>
    <div v-else class="min-h-0 flex-1 overflow-y-auto">
      <div class="border-b border-default px-4 py-3">
        <h2 class="text-base font-semibold">{{ current.name }}</h2>
        <p v-if="current.description" class="text-sm text-muted">{{ current.description }}</p>
      </div>

      <pre v-if="reading === 'rendering' && hasPersona" class="p-4 font-mono text-sm whitespace-pre-wrap select-text">{{ rendering }}</pre>

      <template v-else>
        <ol class="flex flex-col">
          <li v-for="(piece, index) in pieces" :key="piece.id" class="flex items-start gap-2 border-b border-default px-4 py-3">
            <span class="w-8 shrink-0 text-xs text-muted tabular-nums">{{ piece.position }}</span>
            <button type="button" class="min-w-0 flex-1 text-start text-sm whitespace-pre-wrap hover:text-primary" @click="emit('open', piece.id)">{{ piece.text }}</button>
            <span class="shrink-0 text-xs text-muted">{{ piece.type }}</span>
            <div class="flex shrink-0 flex-col">
              <UButton size="xs" color="neutral" variant="ghost" icon="i-material-symbols-keyboard-arrow-up" :aria-label="t('brain.action.up')" :disabled="index === 0 || busy" @click="swap(index, index - 1)" />
              <UButton size="xs" color="neutral" variant="ghost" icon="i-material-symbols-keyboard-arrow-down" :aria-label="t('brain.action.down')" :disabled="index === pieces.length - 1 || busy" @click="swap(index, index + 1)" />
            </div>
          </li>
        </ol>
        <p v-if="pieces.length === 0" class="p-6 text-center text-sm text-muted">{{ t("brain.status.nothing") }}</p>
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useIsland } from "bmo-ui/app/domain/island";
import * as api from "../domain/brain-api";

import type { Layer, Memory } from "../domain/brain-api";

const props = defineProps<{ brainId: number; layers: Layer[]; layerId: number | null; changes: number }>();

const emit = defineEmits<{ layer: [id: number]; open: [id: number]; changed: [] }>();

const { t } = useI18n();
const island = useIsland(t);

const pieces = ref<Memory[]>([]);
const rendering = ref("");
const reading = ref<"pieces" | "rendering">("pieces");
const busy = ref(false);

const current = computed(() => props.layers.find((layer) => layer.id === props.layerId) ?? props.layers[0]);
const layerItems = computed(() => props.layers.map((layer) => ({ value: layer.id, label: layer.name })));
const hasPersona = computed(() => pieces.value.some((piece) => piece.type === "persona" && piece.status === "active"));
const readings = computed(() => [
  { value: "pieces" as const, label: t("brain.label.pieces") },
  { value: "rendering" as const, label: t("brain.label.rendering") }
]);
const exportHref = computed(() => (current.value === undefined ? undefined : api.exportAddress(props.brainId, current.value.name)));

async function read(): Promise<void> {
  const layer = current.value;

  if (layer === undefined) return;

  try {
    pieces.value = (await api.memoryPage(props.brainId, { layerId: layer.id, status: "active,pending" }, "position", null, 500)).items;
    rendering.value = hasPersona.value ? await api.persona(props.brainId, layer.name) : "";
  } catch (error) {
    island.fail(error, t("brain.error.read"));
  }
}

watch(() => [props.brainId, current.value?.id, props.changes], () => void read(), { immediate: true });

// Two pieces trade places: each is written again at the other's position, as a new version.
async function swap(from: number, to: number): Promise<void> {
  const a = pieces.value[from];
  const b = pieces.value[to];

  if (a === undefined || b === undefined) return;

  if ((a.type === "persona" || b.type === "persona") && !(await island.ask({ title: t("brain.hint.persona_cache"), confirm: t("common.action.save"), tone: "warning" }))) return;

  busy.value = true;

  try {
    const [aPosition, bPosition] = a.position === b.position ? [b.position + (to > from ? 1 : -1), a.position] : [b.position, a.position];

    await api.writeVersion(props.brainId, a.id, { position: aPosition });
    await api.writeVersion(props.brainId, b.id, { position: bPosition });
    emit("changed");
  } catch (error) {
    island.fail(error, t("brain.error.save"));
  } finally {
    busy.value = false;
  }
}
</script>
