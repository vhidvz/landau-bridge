<script setup lang="ts">
import type { BridgeError, Dashboard, JobRow, SettingsView } from '~/types/bridge';

const toast = useToast();
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
  if (d.recent_errors > 0) rows.push({ key: 'errors', tone: 'warn', text: `${faNumber(d.recent_errors)} خطا یا هشدار در هفت روز گذشته`, to: '/errors', action: 'مشاهده' });
  return rows;
});
const lastRuns = computed(() => [
  { label: 'آخرین وارد کردن', job: dash.value?.last_import ?? null },
  { label: 'آخرین به‌روزرسانی قیمت', job: dash.value?.last_refresh ?? null },
]);
const statusColor = (j: JobRow) => (j.status === 'DONE' ? 'success' : j.status === 'FAILED' ? 'error' : 'neutral');

// Linux AppImage only: offer to put the file in the applications menu (README, "Other Linux").
const installing = ref(false);
const installAppImage = async () => {
  error.value = null; installing.value = true;
  try {
    const path = await bridge.appimageInstall();
    toast.add({ title: 'در فهرست برنامه‌ها نصب شد', description: path, color: 'success' });
    await load();
  } catch (e) { error.value = e as BridgeError; } finally { installing.value = false; }
};
const dismissAppImage = async () => {
  try { await bridge.appimageDismiss(); await load(); } catch (e) { error.value = e as BridgeError; }
};
</script>

<template>
  <div>
    <ErrorBanner :error="error" />

    <template v-if="dash">
      <PageHeader kicker="داشبورد" :title="dash.store_name ?? 'پل لاندا'" lede="پل بین فاکتورهای بارزپخش و قفسهٔ لاندا" />
      <SetupChecklist v-if="!dash.settings_complete && settings" :settings="settings" />
      <GlassCard v-else><p class="text-paper-dim">داشبورد در حال بازسازی است.</p></GlassCard>

      <section v-if="dash.appimage_offer" class="surface mt-6 flex flex-wrap items-center gap-x-4 gap-y-3 px-5 py-4">
        <UIcon name="i-lucide-package-plus" class="size-6 shrink-0 text-primary" aria-hidden="true" />
        <div class="min-w-0 flex-1">
          <p class="font-medium text-highlighted">برنامه از فایل AppImage اجرا شده است</p>
          <p class="text-sm text-muted">می‌خواهید در فهرست برنامه‌ها نصب شود؟ نسخه‌ای از فایل در پوشهٔ خانگی شما (<code dir="ltr">~/.local/bin</code>) نگه داشته می‌شود و میان‌بر آن به منوی برنامه‌ها اضافه می‌شود.</p>
        </div>
        <div class="flex gap-2">
          <UButton size="sm" icon="i-lucide-download" :loading="installing" @click="installAppImage">نصب</UButton>
          <UButton size="sm" variant="ghost" color="neutral" :disabled="installing" @click="dismissAppImage">نه، دیگر نپرس</UButton>
        </div>
      </section>

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
            <UButton :to="`/jobs/${r.job.id}`" size="xs" variant="ghost" class="ms-auto">جزئیات</UButton>
          </template>
          <span v-else class="text-sm text-muted">هنوز اجرا نشده</span>
        </div>
      </section>
    </template>
    <PageHeader v-else kicker="داشبورد" title="پل لاندا" lede="پل بین فاکتورهای بارزپخش و قفسهٔ لاندا" />
  </div>
</template>
