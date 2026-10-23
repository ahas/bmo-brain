<template>
  <!-- A memory's status as one small badge, coloured the same wherever it stands. -->
  <UBadge :color="color" variant="soft" size="sm" :label="t(`brain.status_name.${status}`)" class="w-fit" />
</template>

<script setup lang="ts">
import type { MemoryStatus } from "../domain/brain-api";

const props = defineProps<{ status: MemoryStatus }>();

const { t } = useI18n();

const color = computed(() => {
  switch (props.status) {
    case "active":
      return "success" as const;
    case "pending":
      return "warning" as const;
    case "invalidated":
      return "error" as const;
    case "done":
      return "info" as const;
    default:
      return "neutral" as const;
  }
});
</script>
