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

// A review can hold hundreds of parts (2026-10-07: 490); a page at a time keeps it workable, and a
// part decided off the end of the last page drops the view onto the new last page.
const PAGE = 20;
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
  try { extra[item.id] = await bridge.reviewSearch(term); } catch (e) { error.value = e as BridgeError; } finally { searching.value = null; }
};
</script>

<template>
  <div class="space-y-4">
    <ErrorBanner :error="error" />
    <p class="max-w-[58ch] text-sm text-paper-mute">برای هر قلم، قطعهٔ درست را از فهرست لاندا انتخاب کنید. «هیچ‌کدام» یعنی این قطعه در لاندا نیست: دیگر پرسیده نمی‌شود و باید برای افزودنش تیکتی برای پشتیبانی لاندا ثبت کنید. «بعداً» آن را برای اجرای بعدی نگه می‌دارد.</p>
    <Pager v-if="items.length > PAGE" v-model:page="page" :total="items.length" :items-per-page="PAGE" class="flex justify-center" />
    <GlassCard v-for="(item, n) in rows" :key="item.id" :pad="false">
      <div class="flex flex-wrap items-center justify-between gap-3 border-b hairline px-6 py-4">
        <div class="min-w-0">
          <h3 class="display text-[1.0625rem] text-paper">{{ item.name }}</h3>
          <p class="tabular text-xs text-paper-mute"><bdi dir="ltr" class="whitespace-nowrap">{{ item.code }}</bdi> · تعداد {{ faNumber(item.qty) }} · {{ faNumber(offset + n + 1) }} از {{ faNumber(items.length) }}</p>
        </div>
        <div class="flex items-center gap-2">
          <USelect v-model="auth[item.id]" :items="[{ label: 'اصلی', value: 'ORIGINAL' }, { label: 'غیراصلی (کپی)', value: 'FAKE' }]" :placeholder="defaultAuthenticity === 'ORIGINAL' ? 'اصلی (پیش‌فرض)' : 'غیراصلی (پیش‌فرض)'" size="sm" class="w-40" />
          <button type="button" class="btn-ghost btn-sm" @click="decide(item, { later: true })">بعداً</button>
          <button type="button" class="btn-danger btn-sm" @click="decide(item, { none: true })">هیچ‌کدام</button>
        </div>
      </div>
      <div class="space-y-3 px-6 py-5">
        <div class="flex gap-2">
          <input v-model="terms[item.id]" class="field flex-1" placeholder="جست‌وجوی دوباره در لاندا با عبارت دیگر" @keyup.enter="research(item)" />
          <UButton :loading="searching === item.id" color="neutral" variant="outline" icon="i-lucide-search" @click="research(item)">جست‌وجو</UButton>
        </div>
        <p v-if="!candidatesOf(item).length" class="text-sm text-paper-mute">لاندا پیشنهادی نداشت. عبارت دیگری را امتحان کنید یا «هیچ‌کدام» را بزنید.</p>
        <ul v-else class="divide-y divide-white/5">
          <li v-for="c in candidatesOf(item)" :key="c.candidate.id" class="flex items-center justify-between gap-3 py-2.5">
            <div class="min-w-0">
              <p class="truncate text-paper">{{ c.candidate.name }} <span v-if="c.candidate.brand" class="text-xs text-paper-mute">{{ c.candidate.brand }}</span></p>
              <p v-if="c.candidate.alias" class="text-xs text-paper-mute" dir="auto">{{ c.candidate.alias }}</p>
            </div>
            <div class="flex shrink-0 items-center gap-3">
              <SimilarityBar :value="c.similarity" />
              <button type="button" class="btn-primary btn-sm" @click="decide(item, { product_id: c.candidate.id, product_name: c.candidate.name, similarity: c.similarity })">همین است</button>
            </div>
          </li>
        </ul>
      </div>
    </GlassCard>
    <Pager v-if="items.length > PAGE" v-model:page="page" :total="items.length" :items-per-page="PAGE" class="flex justify-center" />
  </div>
</template>
