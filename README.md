# پل لندو (Landau Bridge)

برنامه‌ای برای فروشنده خرده‌فروش لندو که دو کار می‌کند:

1. **وارد کردن فاکتورهای بارزپخش روی قفسه لندو** — همه فاکتورهای پرداخت‌شده تا امروز خوانده می‌شود، هر قلم با جست‌وجوی فازی لندو به قطعه درست وصل می‌شود (اقلام مبهم را خودتان تأیید می‌کنید و تصمیم شما ذخیره می‌ماند)، قیمت خرید = قیمت روز بارزپخش با ارزش افزوده، و قیمت قفسه با **قواعد قیمت‌گذاری خودتان در لندو** محاسبه می‌شود. دو حالت: «با تعداد، از تاریخ» یا «با تعداد صفر برای بازبینی».
2. **به‌روزرسانی قیمت‌ها** — هر وقت بخواهید، قیمت خرید ردیف‌های قفسه از قیمت روز بارزپخش تازه می‌شود و قواعد لندو دوباره اعمال می‌شود.

## راه‌اندازی
1. در لندو: حساب کاربری ← کلیدهای API ← یک کلید بسازید (فقط یک بار نمایش داده می‌شود).
2. برنامه را باز کنید ← **تنظیمات**: نشانی درگاه لندو و کلید را وارد کنید، «بررسی اتصال» را بزنید، فروشگاه مقصد را انتخاب کنید؛ نام کاربری و کلمه عبور بارزپخش را وارد و «آزمایش ورود» کنید؛ ذخیره.
3. اگر در لندو قاعده قیمت‌گذاری ندارید، ابتدا یکی بسازید — وگرنه قیمت قفسه برابر قیمت خرید خواهد بود (برنامه هشدار می‌دهد).

## نکته‌ها
- قیمت‌هایی که خودتان در لندو دستی تایپ کرده باشید حفظ می‌شود، مگر گزینه «بازنویسی قیمت‌های دستی» را روشن کنید.
- هر فاکتور فقط یک بار به موجودی اضافه می‌شود؛ اجرای دوباره امن است.
- قطعه‌ای که در فهرست لندو نیست: «هیچ‌کدام» را بزنید و برای افزودن آن تیکتی برای پشتیبانی لندو ثبت کنید.
- همه خطاها و هشدارها در صفحه «گزارش و خطاها» و در پوشه لاگ ثبت می‌شوند.

---

## For developers
Tauri 2 + Nuxt 4 + Rust core. Design: `docs/superpowers/specs/2026-09-13-landau-bridge-design.md`. Plan: `docs/superpowers/plans/2026-09-13-landau-bridge.md`. Live testing and fixtures: `docs/RUNBOOK.md`. Matching threshold and why: `docs/matching-threshold.md`.

Visual design: `docs/superpowers/specs/2026-09-14-ui-redesign-design.md`. To look at the pages without Tauri: `pnpm dev`, open `http://localhost:3210/?state=done&mode=dark` (`state` = `empty|running|review|refreshing|done`; fixtures in `app/dev/mockBridge.ts`); `scripts/shots.sh OUT_DIR` screenshots every page in both modes.

```sh
pnpm install
cd src-tauri && cargo test && cd ..          # offline
pnpm test && pnpm typecheck                   # UI utils, types
pnpm tauri dev                                # run
pnpm tauri build                              # bundle → src-tauri/target/release/bundle/
```
