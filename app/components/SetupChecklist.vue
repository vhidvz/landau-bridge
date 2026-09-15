<script setup lang="ts">
import type { SettingsView } from '~/types/bridge';

const props = defineProps<{ settings: SettingsView }>();
const steps = computed(() => [
  { label: 'کلید API لاندا', done: props.settings.has_landau_token, hint: 'در لاندا: حساب کاربری ← کلیدهای API؛ کلید فقط یک بار نمایش داده می‌شود' },
  { label: 'فروشگاه مقصد', done: !!props.settings.store, hint: 'بعد از «بررسی اتصال»، فروشگاهی را که ردیف‌ها روی قفسه‌اش نوشته می‌شود انتخاب کنید' },
  { label: 'ورود بارزپخش', done: !!props.settings.supplier_user && props.settings.has_supplier_password, hint: 'نام کاربری و کلمه عبور بارزپخش، سپس «آزمایش ورود»' },
]);
</script>

<template>
  <GlassCard class="flex flex-wrap items-center gap-8 !p-8">
    <BrandLogo :size="96" tone="brand" class="shrink-0" />
    <div class="min-w-0 flex-1">
      <h2 class="display text-2xl text-paper">سه قدم تا اولین عبور</h2>
      <p class="mt-1 max-w-[58ch] text-paper-dim">پل وقتی باز می‌شود که هر دو کرانه وصل باشند.</p>
      <ol class="mt-5 space-y-3">
        <li v-for="(s, i) in steps" :key="s.label" class="flex items-start gap-3">
          <span class="check mt-0.5" :class="s.done && 'is-done'">
            <UIcon v-if="s.done" name="i-lucide-check" class="size-3.5" aria-hidden="true" />
            <template v-else>{{ faNumber(i + 1) }}</template>
          </span>
          <span class="min-w-0">
            <span class="block font-medium text-paper">{{ s.label }}</span>
            <span class="block text-sm text-paper-mute">{{ s.hint }}</span>
          </span>
        </li>
      </ol>
      <NuxtLink to="/settings" class="btn-primary mt-6"><UIcon name="i-lucide-settings" class="size-4" />رفتن به تنظیمات</NuxtLink>
    </div>
  </GlassCard>
</template>
