<template>
  <!-- The review of the base's candidates (docs/design.md, 「The screen」 and 「Raising a memory to the base」): one item
       a candidate, oldest first, its words in the reading language, its type, its status and its source, which opens
       that memory in its own brain, and three decisions at its end. Raising goes through at once; keeping and
       dropping grow a step of the item that asks first, dropping saying that the source is invalidated too. A
       decided candidate leaves the list; a failed decision leaves it standing with one line saying so. -->
  <div class="flex min-h-0 flex-1 flex-col">
    <BrainHeading v-bind="heading" :count="candidates === null ? null : candidates.length" />
    <div class="flex min-h-0 flex-1 flex-col overflow-y-auto pb-(--bmo-create-room)" data-review>
      <ContentSkeleton v-if="candidates === null" class="p-3" :rows="6" list-class="flex flex-col gap-2" :label="t('brain.status.loading')">
        <SkeletonBar class="h-16 w-full" />
      </ContentSkeleton>
      <p v-else-if="failure" class="p-4 text-sm text-error">{{ failure }}</p>
      <p v-else-if="candidates.length === 0" class="p-6 text-center text-sm text-muted">{{ t("brain.review.empty") }}</p>

      <MovingList v-else tag="ul" class="flex flex-col">
        <li v-for="candidate in candidates" :key="candidate.id" class="flex flex-col gap-2 border-b border-default px-3 py-3" :data-candidate="candidate.id">
          <div class="flex min-w-0 items-center gap-2">
            <BrainStatus :status="candidate.status" />
            <span class="shrink-0 text-sm text-muted">{{ typeName(t, te, candidate.type) }}</span>
            <UTooltip :text="t('brain.review.open_source')">
              <UButton
                size="sm"
                color="neutral"
                variant="link"
                class="min-w-0 truncate tabular-nums"
                :label="sourceLabel(candidate)"
                data-source
                @click="openSource(candidate)"
              />
            </UTooltip>
            <div class="ms-auto flex shrink-0 items-center gap-1">
              <UTooltip :text="t('brain.review.raise')">
                <UButton
                  size="sm"
                  square
                  color="neutral"
                  variant="ghost"
                  icon="i-material-symbols-public"
                  :aria-label="t('brain.review.raise')"
                  :loading="busy === candidate.id && asking === null"
                  :disabled="busy !== null"
                  data-decide="raise"
                  @click="raise(candidate)"
                />
              </UTooltip>
              <UTooltip :text="keepWords(candidate)">
                <UButton
                  size="sm"
                  square
                  color="neutral"
                  variant="ghost"
                  icon="i-material-symbols-bookmark-outline"
                  :aria-label="keepWords(candidate)"
                  :disabled="busy !== null"
                  data-decide="keep"
                  @click="ask(candidate, 'keep')"
                />
              </UTooltip>
              <UTooltip :text="t('brain.review.drop')">
                <UButton
                  size="sm"
                  square
                  color="neutral"
                  variant="ghost"
                  icon="i-material-symbols-delete-outline"
                  :aria-label="t('brain.review.drop')"
                  :disabled="busy !== null"
                  data-decide="drop"
                  @click="ask(candidate, 'drop')"
                />
              </UTooltip>
            </div>
          </div>
          <BrainText :text="candidate.text" text-class="text-sm whitespace-pre-wrap" />
          <p v-if="candidate.labels?.candidateReason" class="text-sm text-muted">{{ reader.shown(candidate.labels.candidateReason) }}</p>
          <p v-if="failed === candidate.id" class="text-sm text-error">{{ t("brain.review.failed") }}</p>

          <!-- The step the item grows before it keeps or drops: what going through does, the way out first and the
               way through last. -->
          <div v-if="asking !== null && asking.id === candidate.id" class="flex flex-wrap items-center gap-2 rounded-md bg-elevated px-3 py-2" data-asking>
            <p class="min-w-0 flex-1 text-sm">{{ asking.what === "keep" ? t("brain.review.keep_effect", { brain: sourceName(candidate) }) : t("brain.review.drop_effect", { brain: sourceName(candidate) }) }}</p>
            <div class="ms-auto flex shrink-0 items-center gap-2">
              <UButton size="sm" color="neutral" variant="ghost" :label="t('common.action.cancel')" :disabled="busy !== null" @click="asking = null" />
              <UButton
                size="sm"
                :color="asking.what === 'drop' ? 'error' : 'primary'"
                :label="asking.what === 'keep' ? t('brain.review.keep_short') : t('brain.review.drop')"
                :loading="busy === candidate.id"
                data-confirm
                @click="decide(candidate, asking.what)"
              />
            </div>
          </div>
        </li>
      </MovingList>
    </div>
  </div>
</template>

<script setup lang="ts">
import BrainHeading from "./BrainHeading.vue";
import BrainStatus from "./BrainStatus.vue";
import BrainText from "./BrainText.vue";
import * as api from "../domain/brain-api";
import { typeName } from "../domain/brain-names";
import { useReading } from "../domain/brain-reading";
import { DROPPED, KEPT_IN_SOURCE, candidatesOf, sourceOf } from "../domain/brain-review";

import type { Brain, Memory } from "../domain/brain-api";
import type { Heading } from "../domain/brain-heading";

const props = defineProps<{ brainId: number; heading: Heading; changes: number; brains: Brain[] }>();

const emit = defineEmits<{ source: [place: { brainId: number; memoryId: number }] }>();

const { t, te } = useI18n();
const reader = useReading();

const candidates = ref<Memory[] | null>(null);
const failure = ref("");
// The candidate a decision is under way for, the one whose step asks, and the one whose decision failed.
const busy = ref<number | null>(null);
const asking = ref<{ id: number; what: "keep" | "drop" } | null>(null);
const failed = ref<number | null>(null);

async function read(): Promise<void> {
  try {
    candidates.value = candidatesOf(await api.memoriesNamed(props.brainId, { status: "pending" }));
    failure.value = "";
  } catch {
    candidates.value = [];
    failure.value = t("brain.error.read");
  }
}

watch(() => [props.brainId, props.changes], () => void read(), { immediate: true });

// Each candidate's words, and the model's reason, asked for in the reading language as they come.
watch(
  () => [candidates.value, reader.language.value] as const,
  ([shown]) =>
    reader.want((shown ?? []).flatMap((candidate) => [candidate.text, candidate.labels?.candidateReason ?? ""].filter((text) => text !== ""))),
  { immediate: true }
);

function sourceName(candidate: Memory): string {
  const source = sourceOf(candidate);
  const brain = props.brains.find((one) => one.id === source?.brainId);

  return brain?.name || brain?.agentKey || `#${source?.brainId ?? ""}`;
}

function sourceLabel(candidate: Memory): string {
  return t("brain.review.source", { brain: sourceName(candidate), id: (sourceOf(candidate)?.memoryId ?? 0).toLocaleString() });
}

function keepWords(candidate: Memory): string {
  return t("brain.review.keep", { brain: sourceName(candidate) });
}

function openSource(candidate: Memory): void {
  const source = sourceOf(candidate);

  if (source !== null) emit("source", source);
}

function ask(candidate: Memory, what: "keep" | "drop"): void {
  failed.value = null;
  asking.value = { id: candidate.id, what };
}

function leave(candidate: Memory): void {
  candidates.value = (candidates.value ?? []).filter((one) => one.id !== candidate.id);
}

// A decision written: raised to active; kept, the candidate invalidated; dropped, the source invalidated in its own
// brain first, then the candidate, so a source that cannot move leaves the candidate standing.
async function decide(candidate: Memory, what: "raise" | "keep" | "drop"): Promise<void> {
  const source = sourceOf(candidate);

  busy.value = candidate.id;
  failed.value = null;
  try {
    if (what === "raise") await api.moveMemory(props.brainId, candidate.id, "active", null, null);
    if (what === "keep") await api.moveMemory(props.brainId, candidate.id, "invalidated", KEPT_IN_SOURCE, null);
    if (what === "drop" && source !== null) {
      await api.moveMemory(source.brainId, source.memoryId, "invalidated", DROPPED, null);
      await api.moveMemory(props.brainId, candidate.id, "invalidated", DROPPED, null);
    }
    asking.value = null;
    leave(candidate);
  } catch {
    failed.value = candidate.id;
  } finally {
    busy.value = null;
  }
}

function raise(candidate: Memory): void {
  asking.value = null;
  void decide(candidate, "raise");
}
</script>
