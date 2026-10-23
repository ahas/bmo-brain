<template>
  <!-- A text of the brain as the screen reads it (docs/design.md, 「The screen」): its reading in the reading
       language once it has come, saying it is translated, with its English one tap away; the English while the
       reading is on its way, where it failed or came back untranslated, or while the screen reads English. The
       English may be drawn by the slot, as the timeline draws a version's change. -->
  <div class="flex flex-col gap-1">
    <component :is="tag" v-if="reading !== undefined && !original" :class="textClass">{{ reading.text }}</component>
    <slot v-else>
      <component :is="tag" :class="textClass">{{ text }}</component>
    </slot>
    <div v-if="reader.pending(text)" class="flex items-center gap-1 text-xs text-dimmed">
      <UIcon name="i-material-symbols-translate" class="size-3.5 shrink-0" />
      <span>{{ t("brain.status.translating") }}</span>
    </div>
    <div v-else-if="reading !== undefined" class="flex items-center gap-1 text-xs text-muted">
      <UIcon name="i-material-symbols-translate" class="size-3.5 shrink-0" />
      <span>{{ original ? t("brain.label.original") : reading.whole ? t("brain.label.translated") : t("brain.label.translated_in_part") }}</span>
      <UButton
        size="xs"
        color="neutral"
        variant="link"
        :label="original ? t('brain.action.show_translation') : t('brain.action.show_original')"
        :aria-pressed="original"
        @click="original = !original"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { useReading } from "../domain/brain-reading";

const props = withDefaults(defineProps<{ text: string; tag?: string; textClass?: string }>(), { tag: "p", textClass: "" });

const { t } = useI18n();
const reader = useReading();

const original = ref(false);
const reading = computed(() => reader.of(props.text));

watch(
  () => [props.text, reader.language.value],
  () => {
    original.value = false;
    reader.want([props.text]);
  },
  { immediate: true }
);
</script>
