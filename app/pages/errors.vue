<script setup lang="ts">
import type { BridgeError, ErrorRow } from '~/types/bridge';
import type { KindFamily } from '~/utils/fa';

const route = useRoute();
const router = useRouter();
const rows = ref<ErrorRow[]>([]);
const error = ref<BridgeError | null>(null);
const family = ref<'ALL' | KindFamily>('ALL');
const families = [{ value: 'ALL', label: 'همه' }, { value: 'supplier', label: 'بارزپخش' }, { value: 'landau', label: 'لاندا' }, { value: 'app', label: 'برنامه' }] as const;
const query = ref('');
const opened = ref<number | null>(null);

const jobFilter = computed(() => { const n = Number(route.query.job); return Number.isFinite(n) && n > 0 ? n : null; });
const shown = computed(() => rows.value.filter((r) =>
  (family.value === 'ALL' || kindFamily(r.kind) === family.value)
  && (jobFilter.value == null || r.job_id === jobFilter.value)
  && (!query.value || r.message.includes(query.value) || kindLabel(r.kind).includes(query.value))));

const load = async () => { try { rows.value = await bridge.errorsList(300); } catch (e) { error.value = e as BridgeError; } };
onMounted(load);
const clearJob = () => router.replace({ query: {} });
const openFolder = async () => { error.value = null; try { await bridge.openLogFolder(); } catch (e) { error.value = e as BridgeError; } };
const toggle = (e: ErrorRow) => { if (e.context != null) opened.value = opened.value === e.id ? null : e.id; };
</script>

<template>
  <div v-reveal>
    <PageHeader kicker="خطاها و هشدارها · ۰۶" title="هر لغزش، با جزئیات." lede="هر خطا یا هشداری که در اجرا ثبت شده، با نوع، عملیات و جزئیات فنی.">
      <template #actions>
        <UButton color="neutral" variant="outline" icon="i-lucide-rotate-cw" @click="load">تازه‌سازی</UButton>
        <UButton color="neutral" variant="outline" icon="i-lucide-folder-open" @click="openFolder">باز کردن پوشهٔ لاگ</UButton>
      </template>
    </PageHeader>
    <ErrorBanner :error="error" />

    <div class="mb-4 flex flex-wrap items-center gap-3" data-reveal>
      <SegmentedControl v-model="family" :items="families" aria-label="خانوادهٔ خطا" />
      <button
        v-if="jobFilter != null" type="button"
        class="inline-flex items-center gap-1.5 rounded-full border border-crimson-500/35 bg-crimson-500/8 px-2.5 py-[4px] text-[0.6875rem] leading-[12px] font-medium text-crimson-300"
        @click="clearJob"
      >
        فقط عملیات {{ faNumber(jobFilter) }}
        <UIcon name="i-lucide-x" class="size-3" aria-hidden="true" /><span class="sr-only">حذف فیلتر</span>
      </button>
      <span class="tabular text-xs text-paper-mute">{{ faNumber(shown.length) }} مورد</span>
      <input v-model="query" class="field ms-auto w-72" placeholder="جست‌وجو در پیام یا نوع" aria-label="جست‌وجو" />
    </div>

    <LedgerTable v-if="shown.length" class="glass" data-reveal>
      <thead><tr><th>زمان</th><th>عملیات</th><th>نوع</th><th>پیام</th><th></th></tr></thead>
      <tbody>
        <template v-for="e in shown" :key="e.id">
          <tr class="ledger-roomy" :class="e.context != null && 'cursor-pointer'" @click="toggle(e)">
            <td class="whitespace-nowrap">{{ isoToLocal(e.at) }}</td>
            <td>
              <NuxtLink v-if="e.job_id != null" :to="`/jobs/${e.job_id}`" @click.stop><StatusPill tone="muted" :label="faNumber(e.job_id)" /></NuxtLink>
              <span v-else class="text-paper-mute">—</span>
            </td>
            <td class="whitespace-nowrap"><StatusPill tone="muted" :label="kindLabel(e.kind)" /></td>
            <td class="break-words">{{ e.message }}</td>
            <td class="text-end"><UIcon v-if="e.context != null" name="i-lucide-chevron-down" class="size-4 text-paper-mute transition-transform" :class="opened === e.id && 'rotate-180'" /></td>
          </tr>
          <tr v-if="opened === e.id">
            <td colspan="5" class="!border-t-0 !pt-0">
              <div class="glass-inset px-4 py-3">
                <p class="data-label mb-2">جزئیات فنی</p>
                <pre class="code-block" dir="ltr">{{ JSON.stringify(e.context, null, 1) }}</pre>
              </div>
            </td>
          </tr>
        </template>
      </tbody>
    </LedgerTable>
    <GlassCard v-else :pad="false" data-reveal>
      <EmptyState
        :title="rows.length ? 'موردی با این فیلتر نیست' : 'خطا یا هشداری ثبت نشده'"
        :description="rows.length ? 'فیلتر یا عبارت جست‌وجو را عوض کنید.' : 'هر مشکلی در اجرا، با نوع و جزئیاتش، اینجا فهرست می‌شود.'"
      />
    </GlassCard>
  </div>
</template>
