<script setup lang="ts">
import type { BridgeError, ErrorRow } from '~/types/bridge';
import type { KindFamily } from '~/utils/fa';

const route = useRoute();
const router = useRouter();
const rows = ref<ErrorRow[]>([]);
const error = ref<BridgeError | null>(null);
const family = ref<'ALL' | KindFamily>('ALL');
const families = [{ value: 'ALL', label: 'همه' }, { value: 'supplier', label: 'بارزپخش' }, { value: 'landau', label: 'لاندا' }, { value: 'app', label: 'برنامه' }] as const;
const loading = ref(true);
const query = ref('');
const opened = ref<number | null>(null);

const jobFilter = computed(() => { const n = Number(route.query.job); return Number.isFinite(n) && n > 0 ? n : null; });
const shown = computed(() => rows.value.filter((r) =>
  (family.value === 'ALL' || kindFamily(r.kind) === family.value)
  && (jobFilter.value == null || r.job_id === jobFilter.value)
  && matchesQuery(query.value, [r.message, kindLabel(r.kind)])));
const { page, rows: pageRows } = usePaged(shown, PAGE_SIZE.table, [family, query, jobFilter]);
// An opened row stays open only while it is on screen; a new page or filter starts closed.
watch([page, family, query, jobFilter], () => { opened.value = null; });

const load = async () => {
  error.value = null;
  opened.value = null;
  try { rows.value = await bridge.errorsList(300); } catch (e) { error.value = e as BridgeError; } finally { loading.value = false; }
};
onMounted(load);
const clearJob = () => router.replace({ query: { ...route.query, job: undefined } });
const openFolder = async () => { error.value = null; try { await bridge.openLogFolder(); } catch (e) { error.value = e as BridgeError; } };
const toggle = (e: ErrorRow) => { if (e.context != null) opened.value = opened.value === e.id ? null : e.id; };
</script>

<template>
  <div v-reveal>
    <PageHeader title="خطاها و هشدارها" lede="هر مشکلی که در اجراها پیش آمده.">
      <template #actions>
        <UButton color="neutral" variant="outline" icon="i-lucide-rotate-cw" @click="load">تازه‌سازی</UButton>
        <UButton color="neutral" variant="outline" icon="i-lucide-folder-open" @click="openFolder">باز کردن پوشهٔ لاگ</UButton>
      </template>
    </PageHeader>
    <ErrorBanner :error="error" />

    <div class="mb-4 flex flex-wrap items-center gap-3" data-reveal>
      <SegmentedControl v-model="family" :items="families" aria-label="خانوادهٔ خطا" />
      <button v-if="jobFilter != null" type="button" class="inline-flex items-center gap-1.5" @click="clearJob">
        <StatusPill tone="brand" :label="`فقط عملیات ${faNumber(jobFilter)}`" />
        <UIcon name="i-lucide-x" class="size-3" aria-hidden="true" /><span class="sr-only">حذف فیلتر</span>
      </button>
      <div class="relative ms-auto w-72">
        <input v-model="query" type="search" class="field pe-9" placeholder="جست‌وجو در پیام یا نوع" aria-label="جست‌وجو در خطاها" />
        <UIcon name="i-lucide-search" class="pointer-events-none absolute inset-y-0 end-3 my-auto size-4 text-paper-mute" aria-hidden="true" />
      </div>
    </div>

    <LedgerTable v-if="shown.length" class="glass" data-reveal>
      <thead><tr><th>زمان</th><th>عملیات</th><th>نوع</th><th>پیام</th><th></th></tr></thead>
      <tbody>
        <template v-for="e in pageRows" :key="e.id">
          <tr
            class="ledger-roomy" :class="e.context != null && 'cursor-pointer'"
            :tabindex="e.context != null ? 0 : undefined" :aria-expanded="e.context != null ? opened === e.id : undefined"
            @click="toggle(e)" @keydown.enter="toggle(e)"
          >
            <td class="whitespace-nowrap">{{ isoToLocal(e.at) }}</td>
            <td>
              <NuxtLink v-if="e.job_id != null" :to="`/jobs/${e.job_id}`" @click.stop><StatusPill tone="muted" :label="faNumber(e.job_id)" /></NuxtLink>
              <span v-else class="text-paper-mute">—</span>
            </td>
            <td class="whitespace-nowrap"><StatusPill tone="muted" :label="kindLabel(e.kind)" /></td>
            <td class="break-words">{{ e.message }}</td>
            <td class="text-end"><UIcon v-if="e.context != null" name="i-lucide-chevron-down" class="block size-4 text-paper-mute transition-transform" :class="opened === e.id && 'rotate-180'" aria-hidden="true" /></td>
          </tr>
          <tr v-if="opened === e.id">
            <td colspan="5" class="!border-t-0 !pt-0">
              <div class="glass-inset px-4 py-3">
                <p class="data-label mb-2">جزئیات فنی</p>
                <pre class="code-block max-h-80 overflow-auto" dir="ltr">{{ JSON.stringify(e.context, null, 1) }}</pre>
              </div>
            </td>
          </tr>
        </template>
      </tbody>
      <template #footer><Pager v-model:page="page" :total="shown.length" :items-per-page="PAGE_SIZE.table" label="مورد" /></template>
    </LedgerTable>
    <GlassCard v-else-if="!loading && !error" :pad="false" data-reveal>
      <EmptyState
        :title="rows.length ? 'موردی با این فیلتر نیست' : 'خطا یا هشداری ثبت نشده'"
        :description="rows.length ? 'فیلتر یا عبارت جست‌وجو را عوض کنید.' : 'هر مشکلی در اجراها اینجا می‌آید.'"
      />
    </GlassCard>
  </div>
</template>
