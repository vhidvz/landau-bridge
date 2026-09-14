<script setup lang="ts">
import type { BridgeError, Dashboard, JobRow, SettingsView } from '~/types/bridge';

const dash = ref<Dashboard | null>(null);
const settings = ref<SettingsView | null>(null);
const error = ref<BridgeError | null>(null);
const load = async () => {
  try {
    const d = await bridge.dashboardGet();
    settings.value = d.settings_complete ? null : await bridge.settingsGet();
    dash.value = d;
  } catch (e) { error.value = e as BridgeError; }
};
onMounted(load);

const attention = computed(() => {
  const rows: { key: string; tone: 'info' | 'warn'; text: string; to: string; action: string }[] = [];
  const d = dash.value;
  if (!d) return rows;
  if (d.running?.status === 'REVIEW') rows.push({ key: 'review', tone: 'info', text: `${faNumber(d.pending_review)} قلم منتظر تصمیم شماست`, to: `/import?job=${d.running.id}`, action: 'ادامهٔ بازبینی' });
  if (d.recent_errors > 0) rows.push({ key: 'errors', tone: 'warn', text: `${faNumber(d.recent_errors)} خطا یا هشدار در هفت روز گذشته`, to: '/logs', action: 'مشاهده' });
  return rows;
});
const lastRuns = computed(() => [
  { label: 'آخرین وارد کردن', job: dash.value?.last_import ?? null },
  { label: 'آخرین به‌روزرسانی قیمت', job: dash.value?.last_refresh ?? null },
]);
const statusColor = (j: JobRow) => (j.status === 'DONE' ? 'success' : j.status === 'FAILED' ? 'error' : 'neutral');
</script>

<template>
  <div>
    <PageHeader :title="dash?.store_name ?? 'پل لندو'" description="پل بین فاکتورهای بارزپخش و قفسهٔ لندو" />
    <ErrorBanner :error="error" />

    <template v-if="dash">
      <SetupChecklist v-if="!dash.settings_complete && settings" :settings="settings" />
      <BridgeHero v-else :dash="dash" />

      <ul v-if="attention.length" class="surface mt-6 divide-y divide-default">
        <li v-for="a in attention" :key="a.key" class="flex items-center gap-3 px-5 py-3">
          <span class="level-dot !mt-0" :data-level="a.tone === 'warn' ? 'WARN' : 'PRIMARY'" aria-hidden="true" />
          <span class="tabular flex-1">{{ a.text }}</span>
          <UButton :to="a.to" size="sm" variant="soft">{{ a.action }}</UButton>
        </li>
      </ul>

      <section class="surface mt-6 divide-y divide-default">
        <div v-for="r in lastRuns" :key="r.label" class="flex flex-wrap items-center gap-x-4 gap-y-1 px-5 py-3">
          <span class="w-56 shrink-0 text-muted">{{ r.label }}</span>
          <template v-if="r.job">
            <UBadge :color="statusColor(r.job)" variant="soft">{{ statusLabel[r.job.status] ?? r.job.status }}</UBadge>
            <span class="tabular text-sm">{{ isoToLocal(r.job.started_at) }}</span>
            <UButton :to="`/logs?job=${r.job.id}`" size="xs" variant="ghost" class="ms-auto">جزئیات</UButton>
          </template>
          <span v-else class="text-sm text-muted">هنوز اجرا نشده</span>
        </div>
      </section>
    </template>
  </div>
</template>
