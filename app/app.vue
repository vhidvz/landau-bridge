<script setup lang="ts">
import { fa_ir } from '@nuxt/ui/locale';
</script>

<template>
  <UApp :locale="fa_ir" :toaster="{ position: 'bottom-left', duration: 4200 }">
    <NuxtLayout>
      <NuxtPage />
    </NuxtLayout>
  </UApp>
</template>
