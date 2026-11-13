<template>
  <!-- Adding a layer (docs/design.md, 「The screen」): opened from the screen's create button, it takes the new
       layer's name and description and adds it to the brain. -->
  <UModal v-model:open="open" :title="t('brain.action.layer_add')">
    <template #body>
      <div class="flex flex-col gap-3">
        <UFormField :label="t('brain.field.name')">
          <UInput v-model="name" class="w-full" />
        </UFormField>
        <UFormField :label="t('brain.field.description')">
          <UTextarea v-model="description" :rows="3" autoresize class="w-full" />
        </UFormField>
      </div>
    </template>
    <template #footer>
      <div class="flex w-full justify-end gap-2">
        <UButton color="neutral" variant="ghost" :label="t('common.action.cancel')" @click="open = false" />
        <UButton :label="t('common.action.save')" :loading="saving" :disabled="name.trim() === ''" @click="save" />
      </div>
    </template>
  </UModal>
</template>

<script setup lang="ts">
import { useIsland } from "bmo-ui/app/domain/island";
import * as api from "../domain/brain-api";

const props = defineProps<{ brainId: number }>();

const emit = defineEmits<{ added: [] }>();

const open = defineModel<boolean>("open", { required: true });

const { t } = useI18n();
const island = useIsland(t);

const name = ref("");
const description = ref("");
const saving = ref(false);

// Every opening starts from empty fields.
watch(open, (opened) => {
  if (!opened) return;

  name.value = "";
  description.value = "";
});

async function save(): Promise<void> {
  saving.value = true;

  try {
    await api.addLayer(props.brainId, name.value.trim(), description.value);
    open.value = false;
    emit("added");
  } catch (error) {
    island.fail(error, t("brain.error.save"));
  } finally {
    saving.value = false;
  }
}
</script>
