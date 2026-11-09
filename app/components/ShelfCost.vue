<script setup lang="ts">
// A candidate's row as Landau holds it, and what the pick would change: the cost written today
// against the bridge's new cost (plan 130 R-k). Unknown stays unknown.
import type { CandidateView } from '~/types/bridge';

// `failed`: the inspection itself failed, so there is no view to wait for — unknown, not loading.
const props = defineProps<{ view: CandidateView | undefined; failed?: string | null }>();
const row = computed(() => (props.view ? known(props.view.shelf) : undefined));
const failedText = computed(() => (props.view ? unknownText(props.view.shelf) : props.failed ?? null));
// the cost PRICE would write today, the unavailable-only-raises rule applied by the core
const change = computed(() => costChange(props.view?.next_cost, row.value ?? null));
const at = (iso: string) => new Intl.DateTimeFormat('fa-IR', { timeStyle: 'short' }).format(new Date(iso));
</script>

<template>
  <span class="tabular whitespace-nowrap text-xs">
    <span v-if="failedText" class="text-warn" :title="failedText">قفسه: نامعلوم</span>
    <span v-else-if="!view" class="text-paper-mute">…</span>
    <span v-else-if="row === null" class="text-paper-mute">روی قفسهٔ شما نیست</span>
    <span v-else-if="row" :title="`قیمت خرید و فروش آن‌طور که لاندا نگه داشته، خوانده‌شده ساعت ${at(row.read_at)}`">
      <span class="text-paper-mute">روی قفسه {{ money(row.cost) }}</span>
      <span v-if="view?.cost_kept" class="ms-2 text-warn" title="بارزپخش ناموجود و ارزان‌تر است؛ قیمت خرید فعلی می‌ماند">بدون تغییر</span>
      <span v-else-if="change && change.dir !== 'flat'" class="ms-2" :class="change.dir === 'down' ? 'text-danger' : 'text-paper-dim'" :title="`قیمت خرید تازه ${costChangeText(change)}`">{{ costChangeShort(change) }}</span>
    </span>
  </span>
</template>
