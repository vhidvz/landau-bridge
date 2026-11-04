<script setup lang="ts">
import type { Authenticity, BridgeError, JobItemRow, ReviewDecision, Scored } from '~/types/bridge';

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

const decide = async (item: JobItemRow, d: Omit<ReviewDecision, 'item_id'>) => {
  error.value = null;
  try {
    await bridge.reviewDecide({ item_id: item.id, authenticity: auth[item.id] ?? props.defaultAuthenticity, ...d });
    emit('decided', item.id);
  } catch (e) { error.value = e as BridgeError; }
};

const research = async (item: JobItemRow) => {
  const term = (terms[item.id] ?? '').trim();
  if (!term) return;
  searching.value = item.id;
  try { extra[item.id] = await bridge.reviewSearch(term); expanded[item.id] = false; } catch (e) { error.value = e as BridgeError; } finally { searching.value = null; }
};
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
          <USelect v-model="auth[item.id]" :items="[{ label: 'اصلی', value: 'ORIGINAL' }, { label: 'غیراصلی (کپی)', value: 'FAKE' }]" :placeholder="defaultAuthenticity === 'ORIGINAL' ? 'اصلی (پیش‌فرض)' : 'غیراصلی (پیش‌فرض)'" size="sm" class="w-40" />
          <button type="button" class="btn-ghost btn-sm" title="در اجرای بعدی دوباره پرسیده می‌شود" @click="decide(item, { later: true })">بعداً</button>
          <button type="button" class="btn-danger btn-sm" title="دیگر پرسیده نمی‌شود" @click="decide(item, { none: true })">در لاندا نیست</button>
        </div>
      </div>
      <div class="space-y-3 px-6 py-5">
        <div class="flex gap-2">
          <input v-model="terms[item.id]" type="search" class="field flex-1" placeholder="جست‌وجو در لاندا با عبارت دیگر" aria-label="جست‌وجو در لاندا" @keyup.enter="research(item)" />
          <UButton :loading="searching === item.id" color="neutral" variant="outline" icon="i-lucide-search" @click="research(item)">جست‌وجو</UButton>
        </div>
        <p v-if="!candidatesOf(item).length" class="text-sm text-paper-mute">پیشنهادی پیدا نشد؛ عبارت دیگری را جست‌وجو کنید.</p>
        <ul v-else class="divide-y divide-white/5" :class="expanded[item.id] && 'max-h-72 overflow-auto'">
          <li v-for="c in visibleOf(item)" :key="c.candidate.id" class="flex items-center justify-between gap-3 py-2.5">
            <div class="min-w-0">
              <p class="truncate text-paper">{{ c.candidate.name }} <span v-if="c.candidate.brand" class="text-xs text-paper-mute">{{ c.candidate.brand }}</span></p>
              <p v-if="c.candidate.alias" class="text-xs text-paper-mute" dir="auto">{{ c.candidate.alias }}</p>
            </div>
            <div class="flex shrink-0 items-center gap-3">
              <SimilarityBar :value="c.similarity" />
              <button type="button" class="btn-primary btn-sm" @click="decide(item, { product_id: c.candidate.id, product_name: c.candidate.name, similarity: c.similarity })">انتخاب</button>
            </div>
          </li>
        </ul>
        <button v-if="candidatesOf(item).length > TOP" type="button" class="text-xs text-paper-mute hover:text-paper-dim" @click="expanded[item.id] = !expanded[item.id]">
          {{ expanded[item.id] ? 'نمایش کمتر' : `${faNumber(candidatesOf(item).length - TOP)} پیشنهاد دیگر` }}
        </button>
      </div>
    </GlassCard>
    <GlassCard v-if="items.length > PAGE" :pad="false" class="px-[0.875rem] py-[0.625rem]">
      <Pager v-model:page="page" :total="items.length" :items-per-page="PAGE" label="قلم" />
    </GlassCard>
  </div>
</template>
