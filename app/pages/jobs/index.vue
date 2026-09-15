<script setup lang="ts">
import type { BridgeError, JobRow } from '~/types/bridge';

const jobs = ref<JobRow[]>([]);
const error = ref<BridgeError | null>(null);
const filter = ref<'ALL' | 'IMPORT' | 'REFRESH'>('ALL');
const filters = [{ value: 'ALL', label: 'همه' }, { value: 'IMPORT', label: 'وارد کردن' }, { value: 'REFRESH', label: 'به‌روزرسانی' }] as const;
const shown = computed(() => jobs.value.filter((j) => filter.value === 'ALL' || j.kind === filter.value));
const load = async () => { try { jobs.value = await bridge.jobsList(); } catch (e) { error.value = e as BridgeError; } };
onMounted(load);
const open = (j: JobRow) => navigateTo(`/jobs/${j.id}`);
</script>

<template>
  <div v-reveal>
    <PageHeader kicker="عملیات‌ها · ۰۵" title="هر عبور، ثبت‌شده." lede="هر وارد کردن و به‌روزرسانی، با نتیجه و رویدادهایش." />
    <ErrorBanner :error="error" />

    <div class="mb-4 flex items-center gap-3" data-reveal>
      <SegmentedControl v-model="filter" :items="filters" aria-label="نوع عملیات" />
      <span class="tabular text-xs text-paper-mute">{{ faNumber(shown.length) }} عملیات</span>
    </div>

    <LedgerTable v-if="shown.length" class="glass" data-reveal>
      <thead>
        <tr><th>#</th><th>نوع</th><th>شروع</th><th>مدت</th><th>وضعیت</th><th>مرحله</th><th>نتیجه</th><th></th></tr>
      </thead>
      <tbody>
        <tr v-for="j in shown" :key="j.id" class="cursor-pointer" tabindex="0" @click="open(j)" @keydown.enter="open(j)">
          <td class="tabular">{{ faNumber(j.id) }}</td>
          <td class="whitespace-nowrap">{{ jobKindLabel(j) }}<bdi v-if="j.since_date" dir="ltr" class="block text-xs text-paper-mute">{{ j.since_date }}</bdi></td>
          <td class="whitespace-nowrap">{{ isoToLocal(j.started_at) }}</td>
          <td class="whitespace-nowrap">{{ faDuration(j.started_at, j.finished_at) }}</td>
          <td><StatusPill :tone="statusTone(j.status)" :label="statusLabel[j.status] ?? j.status" /></td>
          <td class="whitespace-nowrap">{{ phaseLabel[j.phase] ?? j.phase }}</td>
          <td class="w-full max-w-0 truncate text-xs">{{ summaryLabel(j) || '—' }}</td>
          <td class="text-end"><UIcon name="i-lucide-chevron-left" class="size-4 text-paper-mute" /></td>
        </tr>
      </tbody>
    </LedgerTable>
    <GlassCard v-else :pad="false" data-reveal>
      <EmptyState title="هنوز عملیاتی اجرا نشده" description="اولین وارد کردن یا به‌روزرسانی اینجا ثبت می‌شود." />
    </GlassCard>
  </div>
</template>
