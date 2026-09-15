<script setup lang="ts">
import type { BridgeError, MatchRow, Scored } from '~/types/bridge';

const rows = ref<MatchRow[]>([]);
const error = ref<BridgeError | null>(null);
const filter = ref<'ALL' | 'AUTO' | 'USER' | 'MAP' | 'NONE'>('ALL');
const query = ref('');
const editing = ref<MatchRow | null>(null);
const term = ref('');
const candidates = ref<Scored[]>([]);
const searching = ref(false);

const load = async () => { try { rows.value = await bridge.matchesList(); } catch (e) { error.value = e as BridgeError; } };
onMounted(load);

const shown = computed(() => rows.value.filter((r) => (filter.value === 'ALL' || r.how === filter.value) && (!query.value || r.name_seen.includes(query.value) || r.code.includes(query.value) || (r.product_name ?? '').includes(query.value))));
const howLabel: Record<string, string> = { AUTO: 'خودکار', USER: 'انتخاب شما', MAP: 'از نگاشت', NONE: 'در لاندا نیست' };
const filters = [{ value: 'ALL', label: 'همه' }, { value: 'AUTO', label: 'خودکار' }, { value: 'USER', label: 'انتخاب شما' }, { value: 'MAP', label: 'از نگاشت' }, { value: 'NONE', label: 'در لاندا نیست' }] as const;

const openEdit = (r: MatchRow) => { editing.value = r; term.value = r.name_seen; candidates.value = []; };
const search = async () => {
  searching.value = true;
  try { candidates.value = await bridge.reviewSearch(term.value); } catch (e) { error.value = e as BridgeError; } finally { searching.value = false; }
};
const pick = async (c: Scored) => {
  if (!editing.value) return;
  error.value = null;
  try {
    await bridge.matchSet(editing.value.name_seen, editing.value.code, c.candidate.id, c.candidate.name);
    editing.value = null; await load();
  } catch (e) { error.value = e as BridgeError; }
};
const setNone = async () => {
  if (!editing.value) return;
  error.value = null;
  try {
    await bridge.matchNone(editing.value.name_seen, editing.value.code);
    editing.value = null; await load();
  } catch (e) { error.value = e as BridgeError; }
};
const clear = async () => {
  if (!editing.value) return;
  error.value = null;
  try {
    await bridge.matchClear(editing.value.name_seen);
    editing.value = null; await load();
  } catch (e) { error.value = e as BridgeError; }
};
</script>

<template>
  <div v-reveal>
    <PageHeader kicker="تطبیق‌ها · ۰۴" title="هر کد، یک قطعه." lede="هر کد بارزپخش یک بار به یک قطعهٔ لاندا وصل می‌شود و دیگر پرسیده نمی‌شود." />
    <ErrorBanner :error="error" />

    <div class="mb-4 flex flex-wrap items-center gap-3" data-reveal>
      <SegmentedControl v-model="filter" :items="filters" aria-label="نوع تطبیق" />
      <span class="tabular text-xs text-paper-mute">{{ faNumber(shown.length) }} تطبیق</span>
      <div class="relative ms-auto w-72">
        <input v-model="query" class="field pe-9" placeholder="جست‌وجو در نام یا کد" aria-label="جست‌وجو در تطبیق‌ها" />
        <UIcon name="i-lucide-search" class="pointer-events-none absolute inset-y-0 end-3 my-auto size-4 text-paper-mute" aria-hidden="true" />
      </div>
    </div>

    <LedgerTable v-if="shown.length" class="glass" data-reveal>
      <thead>
        <tr><th>نام در فاکتور</th><th>کد</th><th>قطعهٔ لاندا</th><th>نحوه</th><th>شباهت</th><th>تاریخ</th><th></th></tr>
      </thead>
      <tbody>
        <tr v-for="r in shown" :key="r.name_key">
          <td class="font-medium text-paper">{{ r.name_seen }}</td>
          <td><bdi dir="ltr" class="whitespace-nowrap">{{ r.code }}</bdi></td>
          <td>{{ r.product_name ?? '—' }}</td>
          <td class="whitespace-nowrap"><StatusPill :tone="r.how === 'NONE' ? 'danger' : r.how === 'USER' ? 'ok' : 'info'" :label="howLabel[r.how] ?? r.how" /></td>
          <td><SimilarityBar v-if="r.similarity != null" :value="r.similarity" /><span v-else class="text-paper-mute">—</span></td>
          <td class="whitespace-nowrap">{{ isoToLocal(r.decided_at) }}</td>
          <td class="text-end"><button type="button" class="btn-ghost btn-sm" @click="openEdit(r)">تغییر</button></td>
        </tr>
      </tbody>
    </LedgerTable>
    <GlassCard v-else :pad="false" data-reveal>
      <EmptyState
        :title="rows.length ? 'تطبیقی با این فیلتر نیست' : 'هنوز تطبیقی ثبت نشده'"
        :description="rows.length ? 'فیلتر یا عبارت جست‌وجو را عوض کنید.' : 'با اولین وارد کردن فاکتورها، تطبیق‌های خودکار و تصمیم‌های شما اینجا جمع می‌شود. برای اقلامی که «در لاندا نیست» علامت خورده‌اند، تیکتی برای پشتیبانی لاندا ثبت کنید و بعد تصمیم را همین‌جا تغییر دهید.'"
      >
        <template v-if="!rows.length" #action><NuxtLink to="/import" class="btn-ghost"><UIcon name="i-lucide-file-input" class="size-4" />وارد کردن فاکتورها</NuxtLink></template>
      </EmptyState>
    </GlassCard>

    <UModal :open="!!editing" title="تغییر تطبیق" @update:open="(v: boolean) => { if (!v) editing = null; }">
      <template #body>
        <div v-if="editing" class="space-y-4">
          <p><span class="font-medium text-paper">{{ editing.name_seen }}</span> <bdi dir="ltr" class="whitespace-nowrap text-sm text-paper-mute">{{ editing.code }}</bdi></p>
          <div class="flex gap-2">
            <input v-model="term" class="field flex-1" aria-label="عبارت جست‌وجو" @keyup.enter="search" />
            <UButton :loading="searching" icon="i-lucide-search" @click="search">جست‌وجو در لاندا</UButton>
          </div>
          <ul class="max-h-72 divide-y divide-white/5 overflow-auto">
            <li v-for="c in candidates" :key="c.candidate.id" class="flex items-center justify-between gap-3 py-2.5">
              <span class="min-w-0 truncate text-paper">{{ c.candidate.name }} <span v-if="c.candidate.brand" class="text-xs text-paper-mute">{{ c.candidate.brand }}</span></span>
              <span class="flex shrink-0 items-center gap-3"><SimilarityBar :value="c.similarity" /><button type="button" class="btn-primary btn-sm" @click="pick(c)">انتخاب</button></span>
            </li>
          </ul>
          <div class="flex flex-wrap justify-between gap-2 border-t hairline pt-4">
            <button type="button" class="btn-danger" @click="setNone">در لاندا نیست</button>
            <button type="button" class="btn-ghost" @click="clear">حذف تصمیم، دوباره پرسیده شود</button>
          </div>
        </div>
      </template>
    </UModal>
  </div>
</template>
