<script setup lang="ts">
import type { BridgeError, ErrorRow, JobDetail, JobRow } from '~/types/bridge';

const jobs = ref<JobRow[]>([]);
const errors = ref<ErrorRow[]>([]);
const detail = ref<JobDetail | null>(null);
const error = ref<BridgeError | null>(null);
const tab = ref<'jobs' | 'errors'>('jobs');

const load = async () => {
  try { [jobs.value, errors.value] = await Promise.all([bridge.jobsList(), bridge.errorsList(300)]); } catch (e) { error.value = e as BridgeError; }
};
onMounted(load);
const open = async (j: JobRow) => {
  error.value = null;
  try { detail.value = await bridge.jobGet(j.id); } catch (e) { error.value = e as BridgeError; }
};
const openFolder = async () => {
  error.value = null;
  try { await bridge.openLogFolder(); } catch (e) { error.value = e as BridgeError; }
};
// The Jalali date is a Latin-digit value; keep it out of the Persian label so it can be
// rendered isolated (`<bdi dir="ltr">`) in the template instead of embedded mid-string.
const kindLabelOf = (j: JobRow) => (j.kind === 'IMPORT' ? `وارد کردن (${j.mode === 'ZERO' ? 'صفر' : 'با تعداد'})` : 'به‌روزرسانی قیمت');
</script>

<template>
  <div class="space-y-6">
    <div class="flex items-center justify-between">
      <h2 class="text-2xl font-bold">گزارش و خطاها</h2>
      <UButton variant="soft" icon="i-lucide-folder-open" @click="openFolder">باز کردن پوشه لاگ</UButton>
    </div>
    <ErrorBanner :error="error" />
    <div class="flex gap-2">
      <UButton size="sm" :variant="tab === 'jobs' ? 'solid' : 'soft'" @click="tab = 'jobs'">عملیات‌ها ({{ faNumber(jobs.length) }})</UButton>
      <UButton size="sm" :variant="tab === 'errors' ? 'solid' : 'soft'" @click="tab = 'errors'">خطاها و هشدارها ({{ faNumber(errors.length) }})</UButton>
      <UButton size="sm" variant="ghost" icon="i-lucide-rotate-cw" @click="load">تازه‌سازی</UButton>
    </div>

    <div v-if="tab === 'jobs'" class="grid gap-6 lg:grid-cols-2">
      <div class="overflow-x-auto rounded-lg border border-neutral-200 dark:border-neutral-800">
        <table class="w-full text-sm">
          <thead class="bg-neutral-50 dark:bg-neutral-900"><tr><th class="p-2">#</th><th class="p-2 text-start">نوع</th><th class="p-2">شروع</th><th class="p-2">وضعیت</th><th class="p-2">مرحله</th></tr></thead>
          <tbody>
            <tr v-for="j in jobs" :key="j.id" class="cursor-pointer border-t border-neutral-200 hover:bg-neutral-50 dark:border-neutral-800 dark:hover:bg-neutral-900" :class="detail?.job.id === j.id && 'bg-primary-50 dark:bg-primary-950'" @click="open(j)">
              <td class="p-2 text-center">{{ faNumber(j.id) }}</td>
              <td class="p-2">{{ kindLabelOf(j) }} <bdi v-if="j.kind === 'IMPORT' && j.mode === 'QUANTITY' && j.since_date" dir="ltr" class="text-xs text-neutral-500">{{ j.since_date }}</bdi></td>
              <td class="p-2 text-center">{{ isoToLocal(j.started_at) }}</td>
              <td class="p-2 text-center"><UBadge :color="j.status === 'DONE' ? 'success' : j.status === 'FAILED' ? 'error' : 'neutral'" variant="soft">{{ statusLabel[j.status] ?? j.status }}</UBadge></td>
              <td class="p-2 text-center">{{ phaseLabel[j.phase] ?? j.phase }}</td>
            </tr>
          </tbody>
        </table>
      </div>
      <UCard v-if="detail">
        <template #header><h3 class="font-semibold">عملیات {{ faNumber(detail.job.id) }} — رویدادها</h3></template>
        <pre v-if="detail.job.summary" dir="ltr" class="mb-3 overflow-auto rounded bg-neutral-100 p-2 text-xs dark:bg-neutral-900">{{ JSON.stringify(detail.job.summary, null, 1) }}</pre>
        <ul class="max-h-96 space-y-1 overflow-auto text-sm">
          <li v-for="e in detail.events" :key="e.id" :class="e.level === 'ERROR' ? 'text-red-700' : e.level === 'WARN' ? 'text-amber-700' : ''">
            <span class="font-mono text-xs text-neutral-500">{{ isoToLocal(e.at) }}</span> [{{ phaseLabel[e.phase] ?? e.phase }}] {{ e.message }}
          </li>
        </ul>
        <div class="mt-4"><ReportTable :items="detail.items" :kind="detail.job.kind" /></div>
      </UCard>
    </div>

    <div v-else class="overflow-x-auto rounded-lg border border-neutral-200 dark:border-neutral-800">
      <table class="w-full text-sm">
        <thead class="bg-neutral-50 dark:bg-neutral-900"><tr><th class="p-2">زمان</th><th class="p-2">عملیات</th><th class="p-2 text-start">نوع</th><th class="p-2 text-start">پیام</th></tr></thead>
        <tbody>
          <tr v-for="e in errors" :key="e.id" class="border-t border-neutral-200 dark:border-neutral-800">
            <td class="p-2 text-center whitespace-nowrap">{{ isoToLocal(e.at) }}</td>
            <td class="p-2 text-center">{{ e.job_id != null ? faNumber(e.job_id) : '—' }}</td>
            <td class="p-2"><UBadge variant="soft" color="neutral">{{ kindLabel(e.kind) }}</UBadge></td>
            <td class="p-2">{{ e.message }}<details v-if="e.context" class="mt-1"><summary class="cursor-pointer text-xs text-neutral-500">جزئیات</summary><pre dir="ltr" class="text-xs">{{ JSON.stringify(e.context, null, 1) }}</pre></details></td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>
