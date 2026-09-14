<script setup lang="ts">
import type { BridgeError, ErrorRow, JobDetail, JobRow } from '~/types/bridge';

const jobs = ref<JobRow[]>([]);
const errors = ref<ErrorRow[]>([]);
const detail = ref<JobDetail | null>(null);
const error = ref<BridgeError | null>(null);
const tab = ref<string>('jobs');
const tabs = computed(() => [
  { value: 'jobs', label: `عملیات‌ها (${faNumber(jobs.value.length)})` },
  { value: 'errors', label: `خطاها و هشدارها (${faNumber(errors.value.length)})` },
]);
const jobStatusColor = (s: string) => (s === 'DONE' ? 'success' : s === 'FAILED' ? 'error' : 'neutral');
const route = useRoute();

const load = async () => {
  try { [jobs.value, errors.value] = await Promise.all([bridge.jobsList(), bridge.errorsList(300)]); } catch (e) { error.value = e as BridgeError; }
};
const open = async (j: JobRow) => {
  error.value = null;
  try { detail.value = await bridge.jobGet(j.id); } catch (e) { error.value = e as BridgeError; }
};
onMounted(async () => {
  await load();
  const wanted = Number(route.query.job);
  const j = jobs.value.find((x) => x.id === wanted);
  if (j) await open(j);
});
const openFolder = async () => {
  error.value = null;
  try { await bridge.openLogFolder(); } catch (e) { error.value = e as BridgeError; }
};
// The Jalali date is a Latin-digit value; keep it out of the Persian label so it can be
// rendered isolated (`<bdi dir="ltr">`) in the template instead of embedded mid-string.
const kindLabelOf = (j: JobRow) => (j.kind === 'IMPORT' ? `وارد کردن (${j.mode === 'ZERO' ? 'صفر' : 'با تعداد'})` : 'به‌روزرسانی قیمت');
</script>

<template>
  <div>
    <PageHeader title="گزارش و خطاها" description="هر عملیات با رویدادها و ردیف‌هایش، و هر خطا یا هشداری که ثبت شده.">
      <template #actions>
        <UButton variant="ghost" icon="i-lucide-rotate-cw" @click="load">تازه‌سازی</UButton>
        <UButton variant="soft" icon="i-lucide-folder-open" @click="openFolder">باز کردن پوشهٔ لاگ</UButton>
      </template>
    </PageHeader>
    <ErrorBanner :error="error" />

    <UTabs v-model="tab" :items="tabs" :content="false" variant="pill" class="mb-4 w-fit" />

    <div v-if="tab === 'jobs'" class="grid items-start gap-6 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.3fr)]">
      <LedgerTable v-if="jobs.length">
        <thead>
          <tr><th>#</th><th>نوع</th><th>شروع</th><th>وضعیت</th><th>مرحله</th></tr>
        </thead>
        <tbody>
          <tr v-for="j in jobs" :key="j.id" class="cursor-pointer" :aria-selected="detail?.job.id === j.id" @click="open(j)">
            <td>{{ faNumber(j.id) }}</td>
            <td>
              {{ kindLabelOf(j) }}
              <bdi v-if="j.kind === 'IMPORT' && j.mode === 'QUANTITY' && j.since_date" dir="ltr" class="block text-xs text-muted">{{ j.since_date }}</bdi>
            </td>
            <td class="whitespace-nowrap">{{ isoToLocal(j.started_at) }}</td>
            <td><UBadge :color="jobStatusColor(j.status)" variant="soft">{{ statusLabel[j.status] ?? j.status }}</UBadge></td>
            <td>{{ phaseLabel[j.phase] ?? j.phase }}</td>
          </tr>
        </tbody>
      </LedgerTable>
      <div v-else class="surface"><EmptyState title="هنوز عملیاتی اجرا نشده" description="اولین وارد کردن یا به‌روزرسانی اینجا ثبت می‌شود." /></div>

      <section v-if="detail" class="surface">
        <div class="surface-head">
          <h2>عملیات <span class="tabular">{{ faNumber(detail.job.id) }}</span></h2>
          <UBadge :color="jobStatusColor(detail.job.status)" variant="soft">{{ statusLabel[detail.job.status] ?? detail.job.status }}</UBadge>
        </div>
        <div class="surface-body space-y-4">
          <ul class="max-h-80 space-y-1.5 overflow-auto text-sm">
            <li v-for="e in detail.events" :key="e.id" class="flex items-start gap-2">
              <span class="level-dot" :data-level="e.level" aria-hidden="true" />
              <span class="min-w-0">
                <span class="tabular text-xs text-muted">{{ isoToLocal(e.at) }} · {{ phaseLabel[e.phase] ?? e.phase }}</span>
                <span class="block break-words" :class="e.level === 'ERROR' ? 'text-error' : e.level === 'WARN' ? 'text-warning' : ''">{{ e.message }}</span>
              </span>
            </li>
          </ul>
          <details v-if="detail.job.summary" class="text-sm">
            <summary class="cursor-pointer text-muted">خلاصهٔ خام</summary>
            <pre dir="ltr" class="mt-2 overflow-auto rounded-control bg-elevated p-3 text-xs">{{ JSON.stringify(detail.job.summary, null, 1) }}</pre>
          </details>
          <ReportTable :items="detail.items" :kind="detail.job.kind" />
        </div>
      </section>
      <div v-else-if="jobs.length" class="surface"><EmptyState title="یک عملیات را انتخاب کنید" description="رویدادها و ردیف‌های آن اینجا نشان داده می‌شود." /></div>
    </div>

    <template v-else>
      <LedgerTable v-if="errors.length">
        <thead>
          <tr><th>زمان</th><th>عملیات</th><th>نوع</th><th>پیام</th></tr>
        </thead>
        <tbody>
          <tr v-for="e in errors" :key="e.id">
            <td class="whitespace-nowrap">{{ isoToLocal(e.at) }}</td>
            <td>{{ e.job_id != null ? faNumber(e.job_id) : '—' }}</td>
            <td><UBadge variant="soft" color="neutral">{{ kindLabel(e.kind) }}</UBadge></td>
            <td>
              {{ e.message }}
              <details v-if="e.context" class="mt-1">
                <summary class="cursor-pointer text-xs text-muted">جزئیات</summary>
                <pre dir="ltr" class="mt-1 overflow-auto rounded-control bg-elevated p-2 text-xs">{{ JSON.stringify(e.context, null, 1) }}</pre>
              </details>
            </td>
          </tr>
        </tbody>
      </LedgerTable>
      <div v-else class="surface"><EmptyState title="خطا یا هشداری ثبت نشده" description="هر مشکلی در اجرا، با نوع و جزئیاتش، اینجا فهرست می‌شود." /></div>
    </template>
  </div>
</template>
