<script setup lang="ts">
import type { Authenticity, BridgeError, CandidateView, Inspection, JobItemRow, ReviewDecision, Scored } from '~/types/bridge';

const props = defineProps<{ items: JobItemRow[]; defaultAuthenticity: Authenticity }>();
const emit = defineEmits<{ decided: [item_id: number] }>();

const error = ref<BridgeError | null>(null);
const searching = ref<number | null>(null);
const extra = reactive<Record<number, Scored[]>>({});
const terms = reactive<Record<number, string>>({});
const auth = reactive<Record<number, Authenticity>>({});

const candidatesOf = (item: JobItemRow): Scored[] => extra[item.id] ?? item.candidates ?? [];
// An item carries up to ten suggestions; the best three are shown, the rest one click away in a
// bounded list, so a card keeps one height and a page stays within twice the window.
const TOP = 3;
const expanded = reactive<Record<number, boolean>>({});
const visibleOf = (item: JobItemRow): Scored[] => (expanded[item.id] ? candidatesOf(item) : candidatesOf(item).slice(0, TOP));

// A review can hold hundreds of parts (2026-10-07: 490); a few cards at a time keep a page within
// twice the window's height, and a decided part leaves its page so the next one moves up into it.
const PAGE = PAGE_SIZE.cards;
const { page, rows, offset } = usePaged(toRef(props, 'items'), PAGE);

// ---- what each pick would be (plan 130 R-k): the evidence, the verdict and the money, read for the
// cards on the page only — one price read per item, one shelf read per shown candidate.
const inspections = reactive<Record<number, Inspection | null>>({});
const inspecting = reactive<Record<number, boolean>>({});
const failed = reactive<Record<number, string | null>>({});
// Only the latest read of an item lands: an older answer (the authenticity switched, a re-search)
// must never show the money of a row the pick will not write.
const seq: Record<number, number> = {};
const authOf = (item: JobItemRow): Authenticity => auth[item.id] ?? props.defaultAuthenticity;
const inspect = async (item: JobItemRow) => {
  const ids = visibleOf(item).map((c) => c.candidate.id);
  const mine = (seq[item.id] = (seq[item.id] ?? 0) + 1);
  failed[item.id] = null;
  if (!ids.length) { inspections[item.id] = null; return; }
  inspecting[item.id] = true;
  try {
    const got = await bridge.reviewInspect(item.name, item.code, ids, authOf(item));
    if (seq[item.id] === mine) inspections[item.id] = got;
  } catch (e) {
    if (seq[item.id] === mine) failed[item.id] = (e as BridgeError).message;
  } finally { if (seq[item.id] === mine) inspecting[item.id] = false; }
};
const viewOf = (item: JobItemRow, id: string): CandidateView | undefined =>
  inspections[item.id]?.candidates.find((c) => c.product.id === id);
watch(rows, (now) => { for (const item of now) if (!(item.id in inspections)) void inspect(item); }, { immediate: true });
const reinspect = (item: JobItemRow) => { delete inspections[item.id]; void inspect(item); };

const { pending, busy, pick, respond } = usePick();

const decide = async (item: JobItemRow, d: Omit<ReviewDecision, 'item_id'>) => {
  error.value = null;
  try {
    await bridge.reviewDecide({ item_id: item.id, authenticity: authOf(item), ...d });
    emit('decided', item.id);
  } catch (e) { error.value = e as BridgeError; }
};

/** A pick: one click when the core finds it consistent; against a conflict, R-d's second confirm. */
const choose = async (item: JobItemRow, c: Scored) => {
  error.value = null;
  try {
    const done = await pick(
      (confirmed) => bridge.reviewDecide({ item_id: item.id, authenticity: authOf(item), product_id: c.candidate.id, product_name: c.candidate.name, similarity: c.similarity, confirmed }),
      { supplierName: item.name, product: { name: c.candidate.name, brand: c.candidate.brand }, candidate: viewOf(item, c.candidate.id) ?? null, inspection: inspections[item.id] ?? null },
    );
    if (done) emit('decided', item.id);
  } catch (e) { error.value = e as BridgeError; }
};

const research = async (item: JobItemRow) => {
  const term = (terms[item.id] ?? '').trim();
  if (!term) return;
  searching.value = item.id;
  try { extra[item.id] = await bridge.reviewSearch(term); expanded[item.id] = false; reinspect(item); } catch (e) { error.value = e as BridgeError; } finally { searching.value = null; }
};
const toggle = (item: JobItemRow) => { expanded[item.id] = !expanded[item.id]; reinspect(item); };
const opened = reactive<Record<string, boolean>>({});
</script>

<template>
  <div class="space-y-4" data-paged>
    <ErrorBanner :error="error" />
    <p class="text-sm text-paper-mute">قطعهٔ درست لاندا را برای هر قلم انتخاب کنید.</p>
    <GlassCard v-for="(item, n) in rows" :key="item.id" :pad="false">
      <div class="flex flex-wrap items-center justify-between gap-3 border-b hairline px-6 py-4">
        <div class="min-w-0">
          <h3 class="display text-[1.0625rem] text-paper">{{ item.name }}</h3>
          <p class="tabular text-xs text-paper-mute"><bdi dir="ltr" class="whitespace-nowrap">{{ item.code }}</bdi> · تعداد {{ faNumber(item.qty) }} · {{ faNumber(offset + n + 1) }} از {{ faNumber(items.length) }}</p>
        </div>
        <div class="flex items-center gap-2">
          <USelect v-model="auth[item.id]" :items="[{ label: 'اصلی', value: 'ORIGINAL' }, { label: 'غیراصلی (کپی)', value: 'FAKE' }]" :placeholder="defaultAuthenticity === 'ORIGINAL' ? 'اصلی (پیش‌فرض)' : 'غیراصلی (پیش‌فرض)'" size="sm" class="w-40" @update:model-value="reinspect(item)" />
          <button type="button" class="btn-ghost btn-sm" title="در اجرای بعدی دوباره پرسیده می‌شود" @click="decide(item, { later: true })">بعداً</button>
          <button type="button" class="btn-danger btn-sm" title="دیگر پرسیده نمی‌شود" @click="decide(item, { none: true })">در لاندا نیست</button>
        </div>
      </div>
      <div class="space-y-3 px-6 py-5">
        <MoneyLine :inspection="inspections[item.id] ?? null" :loading="inspecting[item.id]" :failed="failed[item.id]" />
        <div class="flex gap-2">
          <input v-model="terms[item.id]" type="search" class="field flex-1" placeholder="جست‌وجو در لاندا با عبارت دیگر" aria-label="جست‌وجو در لاندا" @keyup.enter="research(item)" />
          <UButton :loading="searching === item.id" color="neutral" variant="outline" icon="i-lucide-search" @click="research(item)">جست‌وجو</UButton>
        </div>
        <p v-if="!candidatesOf(item).length" class="text-sm text-paper-mute">پیشنهادی پیدا نشد؛ عبارت دیگری را جست‌وجو کنید.</p>
        <ul v-else class="divide-y divide-white/5" :class="expanded[item.id] && 'max-h-96 overflow-auto'">
          <li v-for="c in visibleOf(item)" :key="c.candidate.id" class="py-2.5">
            <div class="flex items-center justify-between gap-3">
              <div class="min-w-0">
                <p class="truncate text-paper">{{ c.candidate.name }} <span v-if="c.candidate.brand" class="text-xs text-paper-mute">{{ c.candidate.brand }}</span></p>
                <p v-if="c.candidate.alias" class="truncate text-xs text-paper-mute" dir="auto">{{ c.candidate.alias }}</p>
              </div>
              <div class="flex shrink-0 items-center gap-3">
                <ShelfCost :view="viewOf(item, c.candidate.id)" :failed="failed[item.id]" />
                <button v-if="viewOf(item, c.candidate.id)" type="button" class="rounded-full" :aria-expanded="!!opened[`${item.id}:${c.candidate.id}`]" title="شواهد" @click="opened[`${item.id}:${c.candidate.id}`] = !opened[`${item.id}:${c.candidate.id}`]">
                  <StatusPill :tone="pickTone(viewOf(item, c.candidate.id)!.verdict)" :label="pickLabel(viewOf(item, c.candidate.id)!.verdict)" />
                </button>
                <SimilarityBar :value="c.similarity" />
                <button type="button" class="btn-primary btn-sm" :disabled="busy" @click="choose(item, c)">انتخاب</button>
              </div>
            </div>
            <div v-if="opened[`${item.id}:${c.candidate.id}`] && viewOf(item, c.candidate.id)" class="glass-inset mt-2 px-4 py-3">
              <EvidencePanel :evidence="viewOf(item, c.candidate.id)!.evidence" />
            </div>
          </li>
        </ul>
        <button v-if="candidatesOf(item).length > TOP" type="button" class="text-xs text-paper-mute hover:text-paper-dim" @click="toggle(item)">
          {{ expanded[item.id] ? 'نمایش کمتر' : `${faNumber(candidatesOf(item).length - TOP)} پیشنهاد دیگر` }}
        </button>
      </div>
    </GlassCard>
    <GlassCard v-if="items.length > PAGE" :pad="false" class="px-[0.875rem] py-[0.625rem]">
      <Pager v-model:page="page" :total="items.length" :items-per-page="PAGE" label="قلم" />
    </GlassCard>
    <PickConfirm :pending="pending" @answer="respond" />
  </div>
</template>
