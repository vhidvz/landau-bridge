<script setup lang="ts">
import type { BridgeError, JobRow } from '~/types/bridge';

const jobs = ref<JobRow[]>([]);
const error = ref<BridgeError | null>(null);
const filter = ref<'ALL' | 'IMPORT' | 'REFRESH'>('ALL');
const filters = [{ value: 'ALL', label: 'همه' }, { value: 'IMPORT', label: 'وارد کردن' }, { value: 'REFRESH', label: 'به‌روزرسانی' }] as const;
const shown = computed(() => jobs.value.filter((j) => filter.value === 'ALL' || j.kind === filter.value));
const { page, rows } = usePaged(shown, PAGE_SIZE.table, [filter]);
const loading = ref(true);
const load = async () => {
  error.value = null;
  try { jobs.value = await bridge.jobsList(); } catch (e) { error.value = e as BridgeError; } finally { loading.value = false; }
};
onMounted(load);
const open = (j: JobRow) => navigateTo(`/jobs/${j.id}`);
</script>

<template>
  <div v-reveal>
    <PageHeader title="عملیات‌ها" lede="سابقهٔ وارد کردن‌ها و به‌روزرسانی‌ها.">
      <template #actions>
        <UButton color="neutral" variant="outline" icon="i-lucide-rotate-cw" @click="load">تازه‌سازی</UButton>
      </template>
    </PageHeader>
    <ErrorBanner :error="error" />

    <div class="mb-4 flex items-center gap-3" data-reveal>
      <SegmentedControl v-model="filter" :items="filters" aria-label="نوع عملیات" />
    </div>

    <LedgerTable v-if="shown.length" class="glass" data-reveal>
      <thead>
        <tr><th>#</th><th>نوع</th><th>از تاریخ</th><th>شروع</th><th>مدت</th><th>وضعیت</th><th>مرحله</th><th>نتیجه</th><th></th></tr>
      </thead>
      <tbody>
        <tr v-for="j in rows" :key="j.id" class="cursor-pointer" tabindex="0" @click="open(j)" @keydown.enter="open(j)">
          <td class="tabular">{{ faNumber(j.id) }}</td>
          <td class="whitespace-nowrap">{{ jobKindLabel(j) }}</td>
          <td class="tabular whitespace-nowrap" :class="!j.since_date && 'text-paper-mute'">{{ j.since_date ? faDigits(j.since_date) : '—' }}</td>
          <td class="whitespace-nowrap">{{ isoToLocal(j.started_at) }}</td>
          <td class="whitespace-nowrap">{{ faDuration(j.started_at, j.finished_at) }}</td>
          <td><StatusPill :tone="statusTone(j.status)" :label="statusLabel[j.status] ?? j.status" /></td>
          <td class="whitespace-nowrap">{{ phaseLabel[j.phase] ?? j.phase }}</td>
          <td class="w-full max-w-0 truncate text-xs" :title="summaryLabel(j) || undefined">{{ summaryLabel(j) || '—' }}</td>
          <td class="text-end"><UIcon name="i-lucide-chevron-left" class="block size-4 text-paper-mute" aria-hidden="true" /></td>
        </tr>
      </tbody>
      <template #footer><Pager v-model:page="page" :total="shown.length" :items-per-page="PAGE_SIZE.table" label="عملیات" /></template>
    </LedgerTable>
    <GlassCard v-else-if="!loading && !error" :pad="false" data-reveal>
      <EmptyState
        :title="jobs.length ? 'عملیاتی از این نوع نیست' : 'هنوز عملیاتی اجرا نشده'"
        :description="jobs.length ? 'نوع دیگری را انتخاب کنید.' : 'اولین وارد کردن یا به‌روزرسانی اینجا ثبت می‌شود.'"
      />
    </GlassCard>
  </div>
</template>
