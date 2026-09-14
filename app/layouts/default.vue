<script setup lang="ts">
import type { JobRow } from '~/types/bridge';

const links = [
  { to: '/', label: 'داشبورد', icon: 'i-lucide-layout-dashboard' },
  { to: '/import', label: 'وارد کردن فاکتورها', icon: 'i-lucide-file-input' },
  { to: '/refresh', label: 'به‌روزرسانی قیمت‌ها', icon: 'i-lucide-refresh-cw' },
  { to: '/matches', label: 'تطبیق‌ها', icon: 'i-lucide-link' },
  { to: '/logs', label: 'گزارش و خطاها', icon: 'i-lucide-scroll-text' },
  { to: '/settings', label: 'تنظیمات', icon: 'i-lucide-settings' },
];
const running = ref<JobRow | null>(null);
const poll = async () => { running.value = await bridge.jobCurrent().catch(() => null); };
let timer: ReturnType<typeof setInterval> | undefined;
onMounted(() => { poll(); timer = setInterval(poll, 3000); });
onBeforeUnmount(() => clearInterval(timer));
const version = useRuntimeConfig().public.version;
</script>

<template>
  <div class="flex h-screen bg-muted text-default">
    <aside class="sidebar flex w-64 shrink-0 flex-col overflow-y-auto">
      <NuxtLink to="/" class="flex items-center px-5 pb-4 pt-6 text-white">
        <BrandLogo :size="32" with-wordmark />
      </NuxtLink>
      <nav class="mt-2 flex flex-col gap-1 px-3" aria-label="بخش‌های برنامه">
        <NuxtLink v-for="l in links" :key="l.to" :to="l.to" class="nav-item" active-class="nav-item-active">
          <UIcon :name="l.icon" class="nav-icon size-5 shrink-0" />
          <span>{{ l.label }}</span>
        </NuxtLink>
      </nav>
      <div class="mt-auto flex flex-col gap-3 p-3">
        <RunningJobCard v-if="running" :job="running" compact />
        <div class="flex items-center justify-between px-2 pb-1">
          <span class="tabular text-xs" :style="{ color: 'var(--sidebar-muted)' }" dir="ltr">v{{ version }}</span>
        </div>
      </div>
    </aside>
    <main class="min-w-0 flex-1 overflow-y-auto">
      <div class="mx-auto max-w-[1100px] px-8 py-8">
        <slot />
      </div>
    </main>
  </div>
</template>
