<script setup lang="ts">
// The one pager under every paged list: «۱ تا ۱۵ از ۴۱۲» beside UPagination's buttons.
//
// UPagination is reka-ui's primitives rendered `as-child`, each hard-coding an English aria-label
// ("Next Page", "Page 3") that no Nuxt UI locale reaches (`fa_ir` has no pagination key). A slot's
// button replaces the primitive's and the child's props win the `mergeProps`, so each control below
// carries its own Persian label. A slot also replaces UPagination's icon, which it picks from the
// locale's `dir` (rtl, via `fa_ir` in app.vue), so the RTL icons are restated here. Every button
// is one fixed circle: a page label's own width («۱» against «۴۰») would make the row uneven.
//
// After a page change the list's top (`[data-paged]`, or the pager's parent) is brought back into
// view: a seller who paged from the bottom pager would otherwise land at the bottom of the next
// page, and a capped table's own scroll would keep its old offset.
const props = withDefaults(defineProps<{ total: number; itemsPerPage: number; label?: string }>(), { label: 'ردیف' });
const page = defineModel<number>('page', { required: true });

const root = ref<HTMLElement | null>(null);
const range = computed(() => pageRange(page.value, props.total, props.itemsPerPage));
const multi = computed(() => props.total > props.itemsPerPage);

watch(page, async () => {
  await nextTick();
  const list = root.value?.closest<HTMLElement>('[data-paged]') ?? root.value?.parentElement;
  if (!list) return;
  list.querySelectorAll<HTMLElement>('.ledger').forEach((el) => { el.scrollTop = 0; });
  if (list.getBoundingClientRect().top < 0) list.scrollIntoView({ block: 'start' });
});
</script>

<template>
  <nav
    v-if="total > 0"
    ref="root"
    aria-label="صفحه‌بندی"
    class="flex flex-wrap items-center justify-between gap-3"
    data-pager
  >
    <p class="tabular text-xs text-paper-mute" aria-live="polite">
      <template v-if="multi">{{ faNumber(range.from) }} تا {{ faNumber(range.to) }} از {{ faNumber(total) }} {{ label }}</template>
      <template v-else>{{ faNumber(total) }} {{ label }}</template>
    </p>
    <UPagination v-if="multi" v-model:page="page" :total="total" :items-per-page="itemsPerPage" size="sm" show-edges :sibling-count="1">
      <template #first><UButton color="neutral" variant="outline" size="sm" class="size-8 justify-center p-0" icon="i-lucide-chevrons-right" aria-label="صفحهٔ اول" /></template>
      <template #prev><UButton color="neutral" variant="outline" size="sm" class="size-8 justify-center p-0" icon="i-lucide-chevron-right" aria-label="صفحهٔ قبل" /></template>
      <template #item="{ item, page: current }">
        <UButton
          v-if="item.type === 'page'"
          :color="current === item.value ? 'primary' : 'neutral'"
          :variant="current === item.value ? 'solid' : 'outline'"
          :label="faNumber(item.value)"
          :aria-label="`صفحهٔ ${faNumber(item.value)}`"
          :aria-current="current === item.value ? 'page' : undefined"
          size="sm"
          class="size-8 justify-center p-0"
        />
      </template>
      <template #ellipsis><span class="flex size-8 items-center justify-center text-paper-mute" aria-hidden="true">…</span></template>
      <template #next><UButton color="neutral" variant="outline" size="sm" class="size-8 justify-center p-0" icon="i-lucide-chevron-left" aria-label="صفحهٔ بعد" /></template>
      <template #last><UButton color="neutral" variant="outline" size="sm" class="size-8 justify-center p-0" icon="i-lucide-chevrons-left" aria-label="صفحهٔ آخر" /></template>
    </UPagination>
  </nav>
</template>
