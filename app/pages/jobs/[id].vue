<script setup lang="ts">
import type { BridgeError, JobDetail, SettingsView } from '~/types/bridge';

const route = useRoute();
const id = computed(() => Number(route.params.id));
const detail = ref<JobDetail | null>(null);
const settings = ref<SettingsView | null>(null);
const error = ref<BridgeError | null>(null);
const job = useJob();

const load = async () => {
  error.value = null;
  try {
    detail.value = await bridge.jobGet(id.value);
    if (detail.value.job.status === 'RUNNING' || detail.value.job.status === 'REVIEW') await job.follow(id.value);
  } catch (e) { error.value = e as BridgeError; }
};
onMounted(async () => {
  settings.value = await bridge.settingsGet().catch(() => null);
  await load();
});
watch(id, load);
// once the followed job finishes, useJob reloads its detail — mirror it here
watch(job.detail, (d) => { if (d) detail.value = d; });

const active = computed(() => job.status.value === 'RUNNING' || job.status.value === 'REVIEW');
const live = computed(() => (active.value ? { phase: job.phase.value, progress: job.progress.value, events: job.events.value, status: job.status.value } : null));
const storeName = computed(() => (settings.value && detail.value && settings.value.store === detail.value.job.store ? settings.value.store_name : null));
const lede = computed(() => {
  const j = detail.value?.job;
  if (!j) return '';
  return j.finished_at
    ? `شروع ${isoToLocal(j.started_at)} · پایان ${isoToLocal(j.finished_at)} · ${faDuration(j.started_at, j.finished_at)}`
    : `در حال اجرا از ${isoToLocal(j.started_at)}`;
});
const openFolder = async () => { error.value = null; try { await bridge.openLogFolder(); } catch (e) { error.value = e as BridgeError; } };
</script>

<template>
  <div v-reveal>
    <PageHeader :kicker="`عملیات · ${faNumber(id)}`" :title="detail ? jobKindLabel(detail.job) : 'عملیات'" :lede="lede">
      <template #actions>
        <UButton to="/jobs" color="neutral" variant="outline" icon="i-lucide-arrow-right">همهٔ عملیات‌ها</UButton>
        <UButton color="neutral" variant="outline" icon="i-lucide-folder-open" @click="openFolder">باز کردن پوشهٔ لاگ</UButton>
      </template>
    </PageHeader>
    <ErrorBanner :error="error" />

    <div v-if="detail" class="space-y-6">
      <JobSheet :job="detail.job" :store-name="storeName" :live="live" data-reveal />

      <GlassCard :pad="false" data-reveal>
        <div class="flex items-center justify-between px-6 pt-5 pb-3">
          <h2 class="display text-xl text-paper">رویدادها</h2>
          <span class="tabular text-xs text-paper-mute">{{ faNumber(detail.events.length) }} رویداد</span>
        </div>
        <div v-if="detail.events.length" class="max-h-96 overflow-auto"><EventTimeline :events="detail.events" :kind="detail.job.kind" /></div>
        <p v-else class="px-6 pb-6 text-sm text-paper-mute">رویدادی ثبت نشده.</p>
      </GlassCard>

      <section data-reveal>
        <h2 class="display mb-3 text-xl text-paper">ردیف‌ها</h2>
        <ReportTable :items="detail.items" :kind="detail.job.kind" />
      </section>

      <details v-if="detail.job.summary" class="text-sm" data-reveal>
        <summary class="cursor-pointer text-xs text-paper-mute hover:text-paper-dim">خلاصهٔ خام</summary>
        <pre class="code-block mt-2" dir="ltr">{{ JSON.stringify(detail.job.summary, null, 1) }}</pre>
      </details>
    </div>
  </div>
</template>
