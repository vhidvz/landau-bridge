<script setup lang="ts" generic="T extends string">
// A radiogroup: one tab stop (the checked option), and the arrow keys move the choice — in RTL
// ArrowLeft goes to the next option, ArrowRight to the previous one.
const props = defineProps<{ items: readonly { value: T; label: string }[]; ariaLabel?: string }>();
const model = defineModel<T>({ required: true });
const group = ref<HTMLElement | null>(null);

const move = (step: number) => {
  const n = props.items.length;
  if (!n) return;
  const at = Math.max(0, props.items.findIndex((i) => i.value === model.value));
  const next = (at + step + n) % n;
  model.value = props.items[next]!.value;
  nextTick(() => group.value?.querySelectorAll<HTMLElement>('[role=radio]')[next]?.focus());
};
</script>

<template>
  <div
    ref="group" class="segmented" role="radiogroup" :aria-label="ariaLabel"
    @keydown.left.prevent="move(1)" @keydown.right.prevent="move(-1)" @keydown.down.prevent="move(1)" @keydown.up.prevent="move(-1)"
  >
    <button
      v-for="i in items" :key="i.value" type="button" role="radio" :aria-checked="i.value === model"
      :tabindex="i.value === model ? 0 : -1" class="segmented-btn" @click="model = i.value"
    >
      {{ i.label }}
    </button>
  </div>
</template>
