<template>
  <!-- Teaching (docs/design.md, 「The screen」): the reader's words go through the write judgement as the
       agent's do. The live memories most like them are shown, at most ten; the reader judges them a
       duplicate, a strengthening, a contradiction or new; one learning with the source `owner` applies it.
       A piece placed at a position in a layer is written into that layer instead. -->
  <UModal v-model:open="open" :title="t('brain.action.teach')" :fullscreen="narrow" :ui="{ content: 'sm:max-w-2xl' }">
    <template #body>
      <div class="flex flex-col gap-3">
        <TabSwitch v-model="mode" :items="modes" :label="t('brain.action.teach')" />
        <UFormField :label="t('brain.field.text')" :help="t('brain.hint.english')">
          <UTextarea v-model="text" :rows="3" autoresize class="w-full" />
        </UFormField>
        <UFormField v-if="words.english.value !== null" :label="t('brain.label.translated_english')" :help="t('brain.hint.translated_english')">
          <UTextarea v-model="words.english.value" :rows="3" autoresize class="w-full" />
        </UFormField>
        <p v-if="words.fault.value !== null" class="text-sm text-warning">{{ t(faultKey(words.fault.value)) }}</p>
        <div class="grid grid-cols-2 gap-3">
          <UFormField :label="t('brain.field.type')">
            <UInput v-model="type" class="w-full" />
          </UFormField>
          <UFormField :label="t('brain.field.importance')">
            <UInput v-model.number="importance" type="number" :min="1" :max="10" class="w-full" />
          </UFormField>
        </div>
        <UFormField v-if="mode === 'learning'" :label="t('brain.field.subjects')" :hint="t('brain.hint.subjects')">
          <UInput v-model="subjectText" class="w-full font-mono text-sm" />
        </UFormField>
        <div v-else class="grid grid-cols-2 gap-3">
          <UFormField :label="t('brain.field.layer')">
            <USelect v-model="layer" :items="layerItems" class="w-full" />
          </UFormField>
          <UFormField :label="t('brain.field.position')" :hint="t('brain.hint.position')">
            <UInput v-model="position" type="number" class="w-full" />
          </UFormField>
        </div>

        <template v-if="mode === 'learning'">
          <div class="flex items-center gap-2">
            <UButton color="neutral" variant="outline" icon="i-material-symbols-search" :label="t('brain.action.similar')" :loading="looking || words.busy.value" :disabled="text.trim() === ''" @click="lookSimilar" />
            <span v-if="candidates !== null" class="text-sm text-muted">{{ t("brain.label.similar_count", { n: candidates.length }) }}</span>
          </div>
          <ul v-if="candidates !== null && candidates.length > 0" class="flex flex-col divide-y divide-default rounded-md border border-default">
            <li v-for="candidate in candidates" :key="candidate.id" class="flex flex-col gap-2 p-2">
              <p class="text-sm">{{ reader.shown(candidate.text) }} <span class="text-sm text-muted">#{{ candidate.id }} · {{ typeName(t, te, candidate.type) }}</span></p>
              <USelect v-model="judged[candidate.id]" :items="judgements" size="xs" class="w-48" />
            </li>
          </ul>
        </template>
      </div>
    </template>
    <template #footer>
      <div class="flex w-full justify-end gap-2">
        <UButton color="neutral" variant="ghost" :label="t('common.action.cancel')" @click="open = false" />
        <UButton :label="t('brain.action.teach')" :loading="busy || words.busy.value" :disabled="text.trim() === '' || (mode === 'learning' && candidates === null)" @click="teach" />
      </div>
    </template>
  </UModal>
</template>

<script setup lang="ts">
import { useIsland } from "bmo-ui/app/domain/island";
import * as api from "../domain/brain-api";
import { faultKey, useEnglishWords, useReading } from "../domain/brain-reading";

import { typeName } from "../domain/brain-names";

import type { Layer, Memory, SubjectKey } from "../domain/brain-api";

const props = defineProps<{ brainId: number; layers: Layer[]; narrow: boolean }>();

const emit = defineEmits<{ taught: [memory: Memory | null] }>();

const open = defineModel<boolean>("open", { required: true });

const { t, te } = useI18n();
const island = useIsland(t);

type Mode = "learning" | "piece";
type Judgement = "unrelated" | "duplicate" | "strengthens" | "contradicts";

const mode = ref<Mode>("learning");
const text = ref("");
const type = ref("fact");
const importance = ref(5);
const subjectText = ref("");
const layer = ref("");
const position = ref("");
const candidates = ref<Memory[] | null>(null);
const judged = reactive<Record<number, Judgement>>({});
const looking = ref(false);
const busy = ref(false);

// Words typed in the console's language are read into English and confirmed before they are taught.
const words = useEnglishWords(() => text.value);
const reader = useReading();

watch(candidates, (found) => reader.want((found ?? []).map((candidate) => candidate.text)));

const modes = computed(() => [
  { value: "learning" as const, label: t("brain.teach.learning") },
  { value: "piece" as const, label: t("brain.teach.piece") }
]);

const judgements = computed(() =>
  (["unrelated", "duplicate", "strengthens", "contradicts"] as const).map((value) => ({ value, label: t(`brain.judgement.${value}`) }))
);

const layerItems = computed(() => props.layers.map((found) => ({ value: String(found.id), label: found.name })));

watch(open, (opened) => {
  if (!opened) return;

  text.value = "";
  candidates.value = null;
  subjectText.value = "";
  position.value = "";
  layer.value = props.layers[0] === undefined ? "" : String(props.layers[0].id);
});

watch(text, () => {
  candidates.value = null;
});

// The subjects written as `type:key`, one to a comma.
function subjectsOf(written: string): SubjectKey[] {
  return written
    .split(",")
    .map((part) => part.trim())
    .filter((part) => part.includes(":"))
    .map((part) => {
      const at = part.indexOf(":");

      return { type: part.slice(0, at).trim(), key: part.slice(at + 1).trim() };
    });
}

async function lookSimilar(): Promise<void> {
  const english = await words.ready();

  if (english === null) return;

  looking.value = true;

  try {
    candidates.value = await api.similar(props.brainId, english, subjectsOf(subjectText.value));

    for (const candidate of candidates.value) judged[candidate.id] = "unrelated";
  } catch (error) {
    island.fail(error, t("brain.error.read"));
  } finally {
    looking.value = false;
  }
}

async function teach(): Promise<void> {
  const english = await words.ready();

  if (english === null) return;

  busy.value = true;

  try {
    if (mode.value === "piece") {
      const written = await api.writeMemory(props.brainId, {
        type: type.value.trim(),
        text: english,
        importance: importance.value,
        layerId: layer.value === "" ? null : Number(layer.value),
        position: position.value === "" ? null : Number(position.value),
        subjectIds: [],
        subjects: [],
        labels: {}
      });

      emit("taught", written);
    } else {
      const ids = (judgement: Judgement): number[] =>
        (candidates.value ?? []).filter((candidate) => judged[candidate.id] === judgement).map((candidate) => candidate.id);
      const learned = await api.learn(props.brainId, {
        type: type.value.trim(),
        text: english,
        subjects: subjectsOf(subjectText.value),
        importance: importance.value,
        confirms: ids("duplicate"),
        strengthens: ids("strengthens")[0] ?? null,
        replaces: ids("contradicts"),
        grounds: [],
        labels: {}
      });

      emit("taught", learned.written ?? null);
    }

    open.value = false;
  } catch (error) {
    island.fail(error, t("brain.error.save"));
  } finally {
    busy.value = false;
  }
}
</script>
