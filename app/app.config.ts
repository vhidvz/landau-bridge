export default defineAppConfig({
  ui: {
    colors: {
      primary: 'firoozeh',
      secondary: 'zaferan',
      success: 'sabz',
      info: 'firoozeh',
      warning: 'zaferan',
      error: 'sorkh',
      neutral: 'petrol',
    },
    button: { slots: { base: 'font-medium' } },
    // Arbitrary values, not the named tokens: tailwind-merge only replaces the theme's
    // `rounded-lg` / `shadow-lg` when it recognises the incoming class as the same group.
    modal: { slots: { content: 'rounded-[0.875rem] shadow-[0_12px_32px_-12px_rgb(11_24_32/0.35)]' } },
    select: { slots: { content: 'shadow-[0_12px_32px_-12px_rgb(11_24_32/0.35)]' } },
  },
});
