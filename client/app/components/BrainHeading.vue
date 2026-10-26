<template>
  <!-- The middle pane's heading (docs/design.md, 「The screen」): what is shown, named exactly — the brain and
       the view, every choice standing, each removable on its own, and the count under them — one press that
       clears every choice, the view's own controls at its end, and one line for each choice the view cannot
       honour. -->
  <div class="shrink-0 border-b border-default px-3 py-2">
    <div class="flex flex-wrap items-center gap-x-2 gap-y-1">
      <h2 class="text-base font-semibold">{{ title }}</h2>
      <UBadge v-if="count !== null" color="neutral" variant="soft" size="md" class="tabular-nums">{{ t("brain.label.shown_count", { n: count.toLocaleString() }) }}</UBadge>
      <UBadge
        v-for="chip in chips"
        :key="chip.choice"
        color="primary"
        variant="subtle"
        size="md"
        class="cursor-pointer"
        :class="{ 'line-through opacity-60': !chip.honoured }"
        trailing-icon="i-material-symbols-close"
        :aria-label="t('brain.action.clear', { what: chip.what })"
        @click="chip.clear()"
      >
        {{ chip.what }}: {{ chip.value }}
      </UBadge>
      <UButton v-if="chips.length > 0" size="xs" color="neutral" variant="link" :label="t('brain.action.clear_all')" @click="clearAll()" />
      <div class="ms-auto flex items-center gap-1">
        <slot name="tools" />
      </div>
    </div>
    <p v-for="line in notes" :key="line" class="mt-1 text-sm text-muted">{{ line }}</p>
  </div>
</template>

<script setup lang="ts">
import type { HeadingChip } from "../domain/brain-heading";

defineProps<{ title: string; count: number | null; chips: HeadingChip[]; notes: string[]; clearAll: () => void }>();

const { t } = useI18n();
</script>
