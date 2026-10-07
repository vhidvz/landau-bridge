import type { Ref, WatchSource } from 'vue';

/** One page of `source` at a time: back to the first page when any of `resetOn` changes (a new
 *  filter or search), and onto the last real page when the list shrinks beneath it. */
export function usePaged<T>(source: Ref<readonly T[]>, size: number, resetOn: WatchSource[] = []) {
  const page = ref(1);
  const total = computed(() => source.value.length);
  const rows = computed(() => pageSlice(source.value, page.value, size));
  const offset = computed(() => (clampPage(page.value, total.value, size) - 1) * size);
  if (resetOn.length) watch(resetOn, () => { page.value = 1; });
  watch(total, (n) => { page.value = clampPage(page.value, n, size); });
  const showIndex = (index: number) => { page.value = pageOfIndex(index, size); };
  return { page, rows, total, offset, size, showIndex };
}
