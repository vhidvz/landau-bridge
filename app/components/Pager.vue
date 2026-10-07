<script setup lang="ts">
// UPagination with the app's Persian digits: its page buttons, as UPagination draws them, but
// numbered through `faNumber` like every other figure in the UI.
defineProps<{ total: number; itemsPerPage: number }>();
const page = defineModel<number>('page', { required: true });
</script>

<template>
  <UPagination v-model:page="page" :total="total" :items-per-page="itemsPerPage">
    <template #item="{ item, page: current }">
      <UButton
        v-if="item.type === 'page'"
        :color="current === item.value ? 'primary' : 'neutral'"
        :variant="current === item.value ? 'solid' : 'outline'"
        :label="faNumber(item.value)"
        square
      />
    </template>
  </UPagination>
</template>
