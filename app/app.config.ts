export default defineAppConfig({
  ui: {
    colors: {
      primary: 'crimson', secondary: 'sapphire', success: 'sage', info: 'sapphire',
      warning: 'amber', error: 'rust', neutral: 'graphite',
    },
    // Buttons: solid = hot crimson gradient; outline = ghost; soft error = quiet danger; ghost = text.
    button: {
      slots: { base: 'font-medium rounded-xl whitespace-nowrap transition-all duration-300 [transition-timing-function:var(--ease-swift)] active:translate-y-px' },
      variants: {
        size: {
          sm: { base: 'h-[1.875rem] px-3 py-0 text-[0.8125rem]/[18px] gap-1.5', leadingIcon: 'size-4', trailingIcon: 'size-4' },
          md: { base: 'h-9 px-4 py-0 text-sm/5 gap-2', leadingIcon: 'size-4', trailingIcon: 'size-4' },
          lg: { base: 'h-11 px-5 py-0 text-[0.9375rem]/5 gap-2', leadingIcon: 'size-5', trailingIcon: 'size-5' },
        },
      },
      compoundVariants: [
        { color: 'primary', variant: 'solid', class: 'text-white bg-[linear-gradient(180deg,#cc0e0e,#a30b0b)] shadow-[0_1px_0_rgb(255_255_255/0.25)_inset,0_8px_24px_-8px_rgb(214_62_62/0.5)] hover:bg-[linear-gradient(180deg,#cc0e0e,#a30b0b)] hover:brightness-110 active:bg-[linear-gradient(180deg,#cc0e0e,#a30b0b)] disabled:opacity-45 disabled:bg-[linear-gradient(180deg,#cc0e0e,#a30b0b)]' },
        { color: 'primary', variant: 'outline', class: 'ring-0 border border-line-bright bg-white/2 text-paper-dim hover:bg-white/2 hover:text-paper hover:border-[rgb(224_110_110/0.4)]' },
        { color: 'neutral', variant: 'outline', class: 'ring-0 border border-line-bright bg-white/2 text-paper-dim hover:bg-white/2 hover:text-paper hover:border-[rgb(224_110_110/0.4)]' },
        { color: 'primary', variant: 'soft', class: 'text-crimson-300 bg-[rgb(214_62_62/0.12)] hover:bg-[rgb(214_62_62/0.18)]' },
        { color: 'error', variant: 'soft', class: 'text-danger border border-[rgb(205_116_96/0.35)] bg-[rgb(205_116_96/0.06)] hover:bg-[rgb(205_116_96/0.14)]' },
        { color: 'primary', variant: 'ghost', class: 'text-crimson-400 hover:text-crimson-300 hover:bg-white/4' },
        { color: 'neutral', variant: 'ghost', class: 'text-paper-dim hover:text-paper hover:bg-white/4' },
      ],
    },
    // Fields: the `.field` look, variant "none" so @nuxt/ui's own ring never competes.
    input: {
      slots: { base: 'rounded-xl border border-line-bright bg-ink-900/80 text-paper placeholder:text-paper-mute/60 hover:border-white/28 focus:border-[rgb(224_110_110/0.55)] focus:ring-[3px] focus:ring-[rgb(224_110_110/0.12)] focus-visible:outline-none', leadingIcon: 'text-paper-mute', trailingIcon: 'text-paper-mute' },
      variants: { size: { md: { base: 'h-9 px-3.5 py-0 text-sm/5' } } },
      defaultVariants: { variant: 'none' },
    },
    // If the type check rejects `variant: 'none'` for the select, use `'ghost'` and add `ring-0` to `base`.
    select: {
      slots: {
        base: 'rounded-xl border border-line-bright bg-ink-900/80 text-paper hover:border-white/28 focus:border-[rgb(224_110_110/0.55)] focus-visible:outline-none data-[state=open]:border-[rgb(224_110_110/0.55)]',
        placeholder: 'text-paper-mute/60',
        content: 'glass ring-0 shadow-none rounded-[18px] p-1',
        item: 'rounded-lg text-paper-dim data-highlighted:not-data-disabled:text-paper data-highlighted:not-data-disabled:before:bg-white/6',
        viewport: 'divide-y-0',
      },
      variants: { size: { sm: { base: 'h-[1.875rem] px-3 py-0 text-[0.8125rem]/[18px]' }, md: { base: 'h-9 px-3.5 py-0 text-sm/5' } } },
      defaultVariants: { variant: 'none' },
    },
    formField: {
      slots: { label: 'text-[0.8125rem] font-medium text-paper', hint: 'text-xs text-paper-mute', help: 'text-xs text-paper-mute', description: 'text-xs text-paper-mute', error: 'text-xs text-danger' },
    },
    switch: {
      slots: {
        base: 'data-[state=unchecked]:bg-ink-800 border border-line-bright transition-colors duration-300',
        thumb: 'bg-paper-mute shadow-none data-[state=checked]:bg-crimson-300',
      },
      variants: { color: { primary: { base: 'data-[state=checked]:bg-[rgb(214_62_62/0.3)] data-[state=checked]:border-[rgb(214_62_62/0.6)]' } } },
    },
    modal: {
      slots: {
        overlay: 'bg-ink-950/70 backdrop-blur-sm',
        content: 'glass divide-y divide-white/6 ring-0 shadow-none rounded-[18px]',
        header: 'px-6 py-4', body: 'p-6', footer: 'px-6 py-4',
        title: 'font-display text-lg font-semibold text-paper', description: 'text-xs text-paper-mute',
      },
    },
    progress: { slots: { base: 'bg-ink-700', indicator: 'bg-[linear-gradient(90deg,#d63e3e,#a30b0b)]' } },
    toast: {
      slots: {
        root: "glass ring-0 shadow-none p-4 gap-2.5 items-start before:content-[''] before:mt-1.5 before:size-2 before:shrink-0 before:rounded-full",
        title: 'text-[0.8125rem] font-medium text-paper-dim', description: 'text-xs text-paper-mute',
      },
      variants: {
        color: {
          primary: { root: 'before:bg-crimson-400' }, secondary: { root: 'before:bg-info' }, success: { root: 'before:bg-ok' },
          info: { root: 'before:bg-info' }, warning: { root: 'before:bg-warn' }, error: { root: 'before:bg-danger' }, neutral: { root: 'before:bg-paper-mute' },
        },
      },
    },
  },
});
