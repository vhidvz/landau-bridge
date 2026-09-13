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

const openEdit = (r: MatchRow) => { editing.value = r; term.value = r.name_seen; candidates.value = []; };
const search = async () => {
  searching.value = true;
  try { candidates.value = await bridge.reviewSearch(term.value); } catch (e) { error.value = e as BridgeError; } finally { searching.value = false; }
};
const pick = async (c: Scored) => {
  if (!editing.value) return;
  await bridge.matchSet(editing.value.code, c.candidate.id, c.candidate.name, editing.value.name_seen);
  editing.value = null; await load();
};
const setNone = async () => { if (editing.value) { await bridge.matchNone(editing.value.code, editing.value.name_seen); editing.value = null; await load(); } };
const clear = async () => { if (editing.value) { await bridge.matchClear(editing.value.code); editing.value = null; await load(); } };
</script>

<template>
  <div class="space-y-6">
    <h2 class="text-2xl font-bold">تطبیق‌های ثبت‌شده</h2>
    <ErrorBanner :error="error" />
    <p class="text-sm text-neutral-600">هر کد بارزپخش یک بار به یک قطعه لندو وصل می‌شود و دیگر پرسیده نمی‌شود. برای اقلامی که «در لندو نیست» علامت خورده‌اند، یک تیکت برای پشتیبانی لندو ثبت کنید تا قطعه اضافه شود؛ سپس اینجا تصمیم را تغییر دهید.</p>

    <div class="flex flex-wrap items-center gap-2">
      <UButton v-for="f in (['ALL', 'AUTO', 'USER', 'NONE'] as const)" :key="f" size="sm" :variant="filter === f ? 'solid' : 'soft'" @click="filter = f">
        {{ f === 'ALL' ? 'همه' : howLabel[f] }}
      </UButton>
      <UInput v-model="query" placeholder="جست‌وجو در نام یا کد" class="ms-auto w-64" />
    </div>

    <div class="overflow-x-auto rounded-lg border border-neutral-200 dark:border-neutral-800">
      <table class="w-full text-sm">
        <thead class="bg-neutral-50 dark:bg-neutral-900"><tr><th class="p-2 text-start">نام در فاکتور</th><th class="p-2">کد</th><th class="p-2 text-start">قطعه لندو</th><th class="p-2">نحوه</th><th class="p-2">شباهت</th><th class="p-2">تاریخ</th><th class="p-2"></th></tr></thead>
        <tbody>
          <tr v-for="r in shown" :key="r.code" class="border-t border-neutral-200 dark:border-neutral-800">
            <td class="p-2">{{ r.name_seen }}</td>
            <td class="p-2 text-center" dir="ltr">{{ r.code }}</td>
            <td class="p-2">{{ r.product_name ?? '—' }}</td>
            <td class="p-2 text-center"><UBadge :color="r.how === 'NONE' ? 'error' : r.how === 'USER' ? 'info' : 'success'" variant="soft">{{ howLabel[r.how] }}</UBadge></td>
            <td class="p-2 text-center">{{ r.similarity != null ? `${faNumber(r.similarity)}٪` : '—' }}</td>
            <td class="p-2 text-center">{{ isoToLocal(r.decided_at) }}</td>
            <td class="p-2 text-center"><UButton size="xs" variant="soft" @click="openEdit(r)">تغییر</UButton></td>
          </tr>
        </tbody>
      </table>
    </div>

    <UModal :open="!!editing" title="تغییر تطبیق" @update:open="(v: boolean) => { if (!v) editing = null; }">
      <template #body>
        <div v-if="editing" class="space-y-3">
          <p class="text-sm">{{ editing.name_seen }} <span dir="ltr" class="text-neutral-500">{{ editing.code }}</span></p>
          <div class="flex gap-2">
            <UInput v-model="term" class="flex-1" @keyup.enter="search" />
            <UButton :loading="searching" icon="i-lucide-search" @click="search">جست‌وجو در لندو</UButton>
          </div>
          <ul class="max-h-72 divide-y divide-neutral-200 overflow-auto dark:divide-neutral-800">
            <li v-for="c in candidates" :key="c.candidate.id" class="flex items-center justify-between py-2">
              <span>{{ c.candidate.name }} <span v-if="c.candidate.brand" class="text-xs text-neutral-500">— {{ c.candidate.brand }}</span></span>
              <span class="flex items-center gap-2"><UBadge variant="soft">{{ faNumber(c.similarity) }}٪</UBadge><UButton size="xs" @click="pick(c)">انتخاب</UButton></span>
            </li>
          </ul>
          <div class="flex justify-between">
            <UButton color="error" variant="soft" @click="setNone">در لندو نیست (تیکت)</UButton>
            <UButton color="neutral" variant="soft" @click="clear">حذف تصمیم (دوباره پرسیده شود)</UButton>
          </div>
        </div>
      </template>
    </UModal>
  </div>
</template>
