<template>
  <!-- The last night's sleep (docs/design.md, 「Sleep」 and 「The screen」): the newest live reflection labelled
       `sleep`, folded at the head of the memories whatever the filters, headed by the date it slept for and how long
       ago it was written, its words in the reading language, and a press that opens the row. A brain that has never
       slept shows nothing. -->
  <UCollapsible v-if="slept !== null" v-model:open="open" class="border-b border-default" data-sleep-card>
    <UButton
      color="neutral"
      variant="ghost"
      block
      class="justify-start rounded-none px-3"
      icon="i-material-symbols-bedtime-outline"
      :trailing-icon="open ? 'i-material-symbols-keyboard-arrow-up' : 'i-material-symbols-keyboard-arrow-down'"
      :label="t('brain.label.sleep_of', { date: sleptFor, ago: ago(slept.createdAt) })"
    />
    <template #content>
      <div class="flex flex-col gap-2 px-3 pb-3">
        <BrainText :text="slept.text" tag="p" text-class="text-sm whitespace-pre-wrap select-text" />
        <div>
          <UButton color="neutral" variant="outline" size="sm" :label="t('brain.action.open_sleep')" @click="emit('open', slept.id)" />
        </div>
      </div>
    </template>
  </UCollapsible>
</template>

<script setup lang="ts">
import { useIsland } from "bmo-ui/app/domain/island";
import BrainText from "./BrainText.vue";
import * as api from "../domain/brain-api";
import { SLEEP_LABEL, sleepFilter, sleptOn } from "../domain/brain-sleep";

import type { Memory } from "../domain/brain-api";

const props = defineProps<{ brainId: number; changes: number }>();
const emit = defineEmits<{ open: [id: number] }>();

const { t, locale } = useI18n();
const island = useIsland(t);

const slept = ref<Memory | null>(null);
const open = ref(false);
const sleptFor = computed(() => (slept.value === null ? "" : sleptOn(slept.value, SLEEP_LABEL)));

// How long ago a moment was, in the reader's own words for the span.
function ago(at: number): string {
  const minutes = Math.round((Date.now() - at) / 60_000);
  const say = new Intl.RelativeTimeFormat(locale.value, { numeric: "auto" });
  if (Math.abs(minutes) < 60) return say.format(-minutes, "minute");
  const hours = Math.round(minutes / 60);
  if (Math.abs(hours) < 48) return say.format(-hours, "hour");
  return say.format(-Math.round(hours / 24), "day");
}

async function read(): Promise<void> {
  try {
    const page = await api.memoryPage(props.brainId, sleepFilter(), "desc_validFrom", null, 1);
    slept.value = page.items[0] ?? null;
  } catch (error) {
    slept.value = null;
    island.fail(error, t("brain.error.read"));
  }
}

watch(() => [props.brainId, props.changes], () => void read(), { immediate: true });
</script>
