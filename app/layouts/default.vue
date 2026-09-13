<script setup lang="ts">
const links = [
  { to: '/', label: 'داشبورد', icon: 'i-lucide-layout-dashboard' },
  { to: '/import', label: 'وارد کردن فاکتورها', icon: 'i-lucide-file-input' },
  { to: '/refresh', label: 'به‌روزرسانی قیمت‌ها', icon: 'i-lucide-refresh-cw' },
  { to: '/matches', label: 'تطبیق‌ها', icon: 'i-lucide-link' },
  { to: '/logs', label: 'گزارش و خطاها', icon: 'i-lucide-scroll-text' },
  { to: '/settings', label: 'تنظیمات', icon: 'i-lucide-settings' },
];
const running = ref<{ id: number; kind: string; status: string } | null>(null);
const poll = async () => { running.value = await bridge.jobCurrent().catch(() => null); };
let timer: ReturnType<typeof setInterval> | undefined;
onMounted(() => { poll(); timer = setInterval(poll, 3000); });
onBeforeUnmount(() => clearInterval(timer));
</script>

<template>
  <div class="flex min-h-screen bg-neutral-50 text-neutral-900 dark:bg-neutral-950 dark:text-neutral-100">
    <aside class="w-64 shrink-0 border-e border-neutral-200 bg-white p-4 dark:border-neutral-800 dark:bg-neutral-900">
      <h1 class="mb-6 text-lg font-bold">پل لندو</h1>
      <nav class="space-y-1">
        <NuxtLink v-for="l in links" :key="l.to" :to="l.to" class="flex items-center gap-2 rounded-lg px-3 py-2 hover:bg-neutral-100 dark:hover:bg-neutral-800" active-class="bg-primary-50 text-primary-700 dark:bg-primary-950">
          <UIcon :name="l.icon" class="size-5" />
          <span>{{ l.label }}</span>
        </NuxtLink>
      </nav>
      <div v-if="running" class="mt-6 rounded-lg bg-amber-50 p-3 text-sm text-amber-900 dark:bg-amber-950 dark:text-amber-100">
        عملیات {{ running.kind === 'IMPORT' ? 'وارد کردن' : 'به‌روزرسانی' }} شماره {{ faNumber(running.id) }} —
        {{ statusLabel[running.status] ?? running.status }}
      </div>
    </aside>
    <main class="flex-1 overflow-auto p-6">
      <slot />
    </main>
  </div>
</template>
