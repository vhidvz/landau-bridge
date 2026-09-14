<script setup lang="ts">
import type { BridgeError, MatchRow, Scored } from '~/types/bridge';

const rows = ref<MatchRow[]>([]);
const error = ref<BridgeError | null>(null);
const filter = ref<'ALL' | 'AUTO' | 'USER' | 'NONE'>('ALL');
const query = ref('');
const editing = ref<MatchRow | null>(null);
const term = ref('');
const candidates = ref<Scored[]>([]);
const searching = ref(false);

const load = async () => { try { rows.value = await bridge.matchesList(); } catch (e) { error.value = e as BridgeError; } };
onMounted(load);

const shown = computed(() => rows.value.filter((r) => (filter.value === 'ALL' || r.how === filter.value) && (!query.value || r.name_seen.includes(query.value) || r.code.includes(query.value) || (r.product_name ?? '').includes(query.value))));
const howLabel: Record<string, string> = { AUTO: 'خودکار', USER: 'انتخاب شما', NONE: 'در لندو نیست' };
const filters = [{ value: 'ALL', label: 'همه' }, { value: 'AUTO', label: 'خودکار' }, { value: 'USER', label: 'انتخاب شما' }, { value: 'NONE', label: 'در لندو نیست' }] as const;

const openEdit = (r: MatchRow) => { editing.value = r; term.value = r.name_seen; candidates.value = []; };
const search = async () => {
  searching.value = true;
  try { candidates.value = await bridge.reviewSearch(term.value); } catch (e) { error.value = e as BridgeError; } finally { searching.value = false; }
};
const pick = async (c: Scored) => {
  if (!editing.value) return;
  error.value = null;
  try {
    await bridge.matchSet(editing.value.code, c.candidate.id, c.candidate.name, editing.value.name_seen);
    editing.value = null; await load();
  } catch (e) { error.value = e as BridgeError; }
};
const setNone = async () => {
  if (!editing.value) return;
  error.value = null;
  try {
    await bridge.matchNone(editing.value.code, editing.value.name_seen);
    editing.value = null; await load();
  } catch (e) { error.value = e as BridgeError; }
};
const clear = async () => {
  if (!editing.value) return;
  error.value = null;
  try {
    await bridge.matchClear(editing.value.code);
    editing.value = null; await load();
  } catch (e) { error.value = e as BridgeError; }
};
</script>

<template>
  <div>
    <PageHeader title="تطبیق‌های ثبت‌شده" description="هر کد بارزپخش یک بار به یک قطعهٔ لندو وصل می‌شود و دیگر پرسیده نمی‌شود." />
    <ErrorBanner :error="error" />

    <div class="mb-4 flex flex-wrap items-center gap-3">
      <SegmentedControl v-model="filter" :items="filters" aria-label="نوع تطبیق" />
      <UInput v-model="query" icon="i-lucide-search" placeholder="جست‌وجو در نام یا کد" class="ms-auto w-72" />
    </div>

    <LedgerTable v-if="shown.length">
      <thead>
        <tr>
          <th>نام در فاکتور</th>
          <th>کد</th>
          <th>قطعهٔ لندو</th>
          <th>نحوه</th>
          <th>شباهت</th>
          <th>تاریخ</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="r in shown" :key="r.code">
          <td class="font-medium text-highlighted">{{ r.name_seen }}</td>
          <td><span dir="ltr" class="whitespace-nowrap">{{ r.code }}</span></td>
          <td>{{ r.product_name ?? '—' }}</td>
          <td class="whitespace-nowrap"><UBadge :color="r.how === 'NONE' ? 'error' : r.how === 'USER' ? 'info' : 'success'" variant="soft">{{ howLabel[r.how] }}</UBadge></td>
          <td><SimilarityBar v-if="r.similarity != null" :value="r.similarity" /><span v-else class="text-muted">—</span></td>
          <td class="whitespace-nowrap">{{ isoToLocal(r.decided_at) }}</td>
          <td class="text-end"><UButton size="xs" variant="soft" @click="openEdit(r)">تغییر</UButton></td>
        </tr>
      </tbody>
    </LedgerTable>
    <div v-else class="surface">
      <EmptyState
        :title="rows.length ? 'تطبیقی با این فیلتر نیست' : 'هنوز تطبیقی ثبت نشده'"
        :description="rows.length ? 'فیلتر یا عبارت جست‌وجو را عوض کنید.' : 'با اولین وارد کردن فاکتورها، تطبیق‌های خودکار و تصمیم‌های شما اینجا جمع می‌شود. برای اقلامی که «در لندو نیست» علامت خورده‌اند، تیکتی برای پشتیبانی لندو ثبت کنید و بعد تصمیم را همین‌جا تغییر دهید.'"
      >
        <template v-if="!rows.length" #action><UButton to="/import" variant="soft" icon="i-lucide-file-input">وارد کردن فاکتورها</UButton></template>
      </EmptyState>
    </div>

    <UModal :open="!!editing" title="تغییر تطبیق" @update:open="(v: boolean) => { if (!v) editing = null; }">
      <template #body>
        <div v-if="editing" class="space-y-4">
          <p><span class="font-medium text-highlighted">{{ editing.name_seen }}</span> <span dir="ltr" class="whitespace-nowrap text-sm text-muted">{{ editing.code }}</span></p>
          <div class="flex gap-2">
            <UInput v-model="term" class="flex-1" @keyup.enter="search" />
            <UButton :loading="searching" icon="i-lucide-search" @click="search">جست‌وجو در لندو</UButton>
          </div>
          <ul class="max-h-72 divide-y divide-default overflow-auto">
            <li v-for="c in candidates" :key="c.candidate.id" class="flex items-center justify-between gap-3 py-2.5">
              <span class="min-w-0 truncate">{{ c.candidate.name }} <span v-if="c.candidate.brand" class="text-xs text-muted">{{ c.candidate.brand }}</span></span>
              <span class="flex shrink-0 items-center gap-3"><SimilarityBar :value="c.similarity" /><UButton size="xs" @click="pick(c)">انتخاب</UButton></span>
            </li>
          </ul>
          <div class="flex flex-wrap justify-between gap-2 border-t border-default pt-4">
            <UButton color="error" variant="soft" @click="setNone">در لندو نیست</UButton>
            <UButton color="neutral" variant="ghost" @click="clear">حذف تصمیم، دوباره پرسیده شود</UButton>
          </div>
        </div>
      </template>
    </UModal>
  </div>
</template>
