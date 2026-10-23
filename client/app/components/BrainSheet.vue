<template>
  <!-- A subject's sheet (docs/design.md, 「A person is a subject」): the facts about it as the brain renders
       them, never a row of its own, read in the reading language. It stands over the memories narrowed to the
       subject, folded until opened, and not at all where the subject has no fact. -->
  <UCollapsible v-if="rendering !== ''" v-model:open="open" class="border-b border-default">
    <UButton
      color="neutral"
      variant="ghost"
      block
      class="justify-start rounded-none px-3"
      icon="i-material-symbols-badge-outline"
      :trailing-icon="open ? 'i-material-symbols-keyboard-arrow-up' : 'i-material-symbols-keyboard-arrow-down'"
      :label="t('brain.label.sheet')"
    />
    <template #content>
      <BrainText :text="rendering" tag="pre" text-class="font-sans text-sm whitespace-pre-wrap select-text" class="px-3 pb-3" />
    </template>
  </UCollapsible>
</template>

<script setup lang="ts">
import { useIsland } from "bmo-ui/app/domain/island";
import BrainText from "./BrainText.vue";
import * as api from "../domain/brain-api";

const props = defineProps<{ brainId: number; subjectId: number; changes: number }>();

const { t } = useI18n();
const island = useIsland(t);

const rendering = ref("");
const open = ref(false);

async function read(): Promise<void> {
  try {
    rendering.value = (await api.sheet(props.brainId, props.subjectId)).trim();
  } catch (error) {
    rendering.value = "";
    island.fail(error, t("brain.error.read"));
  }
}

watch(() => [props.brainId, props.subjectId, props.changes], () => void read(), { immediate: true });
</script>
