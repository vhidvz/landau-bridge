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
    <p class="max-w-prose text-sm text-muted">برای هر قلم، قطعهٔ درست را از فهرست لندو انتخاب کنید. «هیچ‌کدام» یعنی این قطعه در لندو نیست: دیگر پرسیده نمی‌شود و باید برای افزودنش تیکتی برای پشتیبانی لندو ثبت کنید. «بعداً» آن را برای اجرای بعدی نگه می‌دارد.</p>
    <section v-for="(item, n) in items" :key="item.id" class="surface">
      <div class="surface-head flex-wrap">
        <div class="min-w-0">
          <h3 class="text-[1.0625rem]">{{ item.name }}</h3>
          <p class="tabular text-xs text-muted"><span dir="ltr">{{ item.code }}</span> · تعداد {{ faNumber(item.qty) }} · {{ faNumber(n + 1) }} از {{ faNumber(items.length) }}</p>
        </div>
        <div class="flex items-center gap-2">
          <USelect v-model="auth[item.id]" :items="[{ label: 'اصلی', value: 'ORIGINAL' }, { label: 'غیراصلی', value: 'FAKE' }]" :placeholder="defaultAuthenticity === 'ORIGINAL' ? 'اصلی (پیش‌فرض)' : 'غیراصلی (پیش‌فرض)'" size="sm" class="w-40" />
          <UButton size="sm" color="neutral" variant="ghost" @click="decide(item, { later: true })">بعداً</UButton>
          <UButton size="sm" color="error" variant="soft" @click="decide(item, { none: true })">هیچ‌کدام</UButton>
        </div>
      </div>
      <div class="surface-body space-y-3">
        <div class="flex gap-2">
          <UInput v-model="terms[item.id]" placeholder="جست‌وجوی دوباره در لندو با عبارت دیگر" class="flex-1" @keyup.enter="research(item)" />
          <UButton :loading="searching === item.id" variant="soft" icon="i-lucide-search" @click="research(item)">جست‌وجو</UButton>
        </div>
        <p v-if="!candidatesOf(item).length" class="text-sm text-muted">لندو پیشنهادی نداشت. عبارت دیگری را امتحان کنید یا «هیچ‌کدام» را بزنید.</p>
        <ul v-else class="divide-y divide-default">
          <li v-for="c in candidatesOf(item)" :key="c.candidate.id" class="flex items-center justify-between gap-3 py-2.5">
            <div class="min-w-0">
              <p class="truncate">{{ c.candidate.name }} <span v-if="c.candidate.brand" class="text-xs text-muted">{{ c.candidate.brand }}</span></p>
              <p v-if="c.candidate.alias" class="text-xs text-muted" dir="auto">{{ c.candidate.alias }}</p>
            </div>
            <div class="flex shrink-0 items-center gap-3">
              <SimilarityBar :value="c.similarity" />
              <UButton size="sm" @click="decide(item, { product_id: c.candidate.id, product_name: c.candidate.name, similarity: c.similarity })">همین است</UButton>
            </div>
          </li>
        </ul>
      </div>
    </section>
  </div>
</template>
