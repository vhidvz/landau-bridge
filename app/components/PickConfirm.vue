<script setup lang="ts">
// R-d's second confirm: a pick against a conflict shows what conflicts, with the values, and what
// the pick would do to the row's cost — and is sent again only on the seller's explicit yes.
import type { PendingConfirm } from '~/composables/usePick';

const props = defineProps<{ pending: PendingConfirm | null }>();
const emit = defineEmits<{ answer: [ok: boolean] }>();
const quote = computed(() => known(props.pending?.inspection?.quote) ?? null);
const row = computed(() => (props.pending?.candidate ? known(props.pending.candidate.shelf) : undefined));
const shelfFailed = computed(() => (props.pending?.candidate ? unknownText(props.pending.candidate.shelf) : null));
const next = computed(() => props.pending?.candidate?.next_cost ?? null);
const change = computed(() => costChange(next.value, row.value ?? null));
</script>

<template>
  <UModal :open="!!pending" title="این قطعه با نام بارزپخش جور نیست" :dismissible="false" @update:open="(v: boolean) => { if (!v) emit('answer', false); }">
    <template #body>
      <div v-if="pending" class="space-y-4">
        <div class="space-y-1 text-sm">
          <p><span class="text-paper-mute">بارزپخش:</span> <span class="text-paper">{{ pending.supplierName }}</span></p>
          <p><span class="text-paper-mute">قطعهٔ لاندا:</span> <span class="text-paper">{{ pending.product.name }}</span> <span class="text-xs text-paper-mute">{{ pending.product.brand }}</span></p>
        </div>
        <ul class="space-y-1.5 rounded-lg border border-danger/30 bg-danger/6 px-4 py-3 text-sm text-paper" aria-label="ناسازگاری‌ها">
          <li v-for="(r, i) in pending.reasons" :key="i" class="flex gap-2"><UIcon name="i-lucide-triangle-alert" class="mt-0.5 size-4 shrink-0 text-danger" aria-hidden="true" />{{ reasonText(r) }}</li>
        </ul>
        <dl class="grid grid-cols-[1fr_auto] gap-x-6 gap-y-1.5 text-sm">
          <dt class="text-paper-mute">قیمت خرید فعلی روی قفسه (لاندا)</dt>
          <dd class="tabular text-end">{{ shelfFailed ? 'نامعلوم' : row === null ? 'روی قفسه نیست' : money(row?.cost) }}</dd>
          <dt class="text-paper-mute">قیمت خرید تازه (بارزپخش با ارزش افزوده)</dt>
          <dd class="tabular text-end">{{ next != null ? money(next) : quote ? money(quote.cost) : 'نامعلوم' }}</dd>
          <dt class="text-paper-mute">تغییر قیمت خرید</dt>
          <dd class="tabular text-end" :class="change?.dir === 'down' ? 'text-danger' : ''">{{ costChangeText(change) }}</dd>
        </dl>
        <p v-if="pending.candidate?.cost_kept" class="text-xs text-warn">بارزپخش این کالا را ناموجود می‌گوید و قیمتش کمتر است؛ قیمت خرید فعلی روی قفسه می‌ماند.</p>
        <p class="text-xs text-paper-mute">قیمت فروش را لاندا پس از نوشتن با قاعدهٔ قیمت‌گذاری شما می‌گذارد.</p>
        <div class="flex justify-end gap-2 border-t hairline pt-4">
          <button type="button" class="btn-ghost" @click="emit('answer', false)">انصراف</button>
          <button type="button" class="btn-danger" @click="emit('answer', true)">با این وجود همین است</button>
        </div>
      </div>
    </template>
  </UModal>
</template>
