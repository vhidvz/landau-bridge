# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Evidence (numbers, words, brand) and a write table that judge every supplier link before it may write. @vhidvz
- Every job reads Landau's whole parts list and proves each supplier link against it before writing. @vhidvz
- One shelf writer is the only way to write a cost or quantity, and refuses a link changed mid-job. @vhidvz
- One app per data folder, and one job at a time even across two processes on one database. @vhidvz
- A held link is labelled in Persian and counted apart from skipped and failed rows. @vhidvz
- Every shelf write is recorded before it is sent and closed only once its outcome is known. @vhidvz
- Each job first settles an earlier write left unknown by reading its row: landed, never landed, or changed. @vhidvz
- A row changed outside the bridge after an unsettled write is held, with both values, until the seller decides. @vhidvz
- Review and Matches show each pick's evidence, today's supplier price, the new cost and the shelf's cost. @vhidvz
- A pick against a conflicting name needs a second confirm showing the conflicts and the cost change. @vhidvz
- The dashboard counts links waiting for review and lists held writes the seller can settle. @vhidvz
- Settings says to run one install per store. @vhidvz
- A shelf audit lists each row whose cost no proven link vouches for, what it should be, and why; CSV export. @vhidvz
- A job's report shows each row's cost before and after the write. @vhidvz
- The 2026-11-06 incident runs end to end offline as a regression test: no wrong cost, both rows held. @vhidvz

### Changed

- A release cut now writes the changelog's compare links, and its notes no longer run into them. @vhidvz
- A link to a product Landau re-imported moves to it only on an exact name, brand and alias. @vhidvz
- Links move to a versioned table, each unverified until a job proves it; old builds find none. @vhidvz
- Two supplier codes with different costs on one shelf row hold it instead of the last write winning. @vhidvz
- Each write reads its shelf row again just before and is planned from what the row holds now. @vhidvz

### Fixed

- A product Landau no longer lists turns its link stale instead of failing every run with a 404. @vhidvz
- A lost attach reply no longer adds the quantity a second time, nor does a crash before its lines are marked. @vhidvz
- A pick in Matches keeps its part's brand and alias, so a re-import of Landau's list no longer drops it. @vhidvz
- A price refresh's summary tiles show its updated, unchanged, held and failed rows, not only the row count. @vhidvz
- The live tests' key script can mint a second key on the same day. @vhidvz

## [1.1.1] - 2026-11-04

### Fixed

- The 1.1.0 notes claimed a pager-arrow fix; the arrows were already right in RTL. @vhidvz

## [1.1.0] - 2026-11-04

### Changed

- Every table pages its rows and stays within twice the window's height, its header pinned. @vhidvz
- The report table fits each row on one line: quantity and price changes beside the value. @vhidvz
- Page headers show the page's name and one short sentence; long explanations are shortened. @vhidvz
- Review shows three items a page, each with its top three suggestions and the rest on demand. @vhidvz
- Review buttons read «در لاندا نیست» and «انتخاب», as on the matches page. @vhidvz
- The dashboard shows three headline tiles for each last run, side by side. @vhidvz

### Fixed

- The pager's buttons are Persian-labelled for screen readers and all one size. @vhidvz
- Search ignores Arabic ي/ك, digit script and ZWNJ differences. @vhidvz
- Start dates show Persian digits, and an item's error no longer repeats its own kind. @vhidvz
- The filter control moves with the arrow keys; jobs and errors no longer flash "empty" on load. @vhidvz

## [1.0.0] - 2026-10-31

### Added

- Releases are built, signed and published by GitHub Actions when main carries a new version. @vhidvz
- Settings shows the app's version, its update status, and a switch, a check button and install-and-restart. @vhidvz
- The app checks GitHub Releases, downloads a newer version in the background and installs it on quit. @vhidvz
- The database is backed up before an upgrade, and an older build refuses data a newer one wrote. @vhidvz
- Invoice pages are fetched in parallel, one per supplier session, 2 to 8 sessions as the link allows. @vhidvz
- Matches lists the parts a run left undecided, with review's saved candidates, to decide at any time. @vhidvz
- Decisions export to a JSON file and import on another install; your picks win, gone products are skipped. @vhidvz
- Long lists are paged: matches and undecided parts 50 rows a page, a review 20 parts a page. @vhidvz

### Changed

- Links the name map made become automatic matches; a decision file's MAP rows import as automatic. @vhidvz
- A shelf row with no link is reported at once and linked once its part arrives on an imported invoice. @vhidvz
- Fewer sessions when added ones stop raising throughput (slow link) or 3 of 24 replies need a retry. @vhidvz
- Supplier requests retry six times over ~1¾ min, including a body cut off mid-download. @vhidvz
- Settings' login check keeps the short budget (three attempts in ~4 s) and answers in seconds. @vhidvz
- An invoice that fails is fetched once more at the end of the pass before it is reported. @vhidvz

### Fixed

- The AppImage no longer bundles libwayland, which left the window blank on a current Mesa. @vhidvz
- The site's error page (its `/Error` redirect answers 200) is retried, not reported as a layout change. @vhidvz
- An invoice the site keeps failing on is reported as `SupplierFaulted`, the site's fault, not its layout. @vhidvz
- README names the supplier host the app calls, `33314466.ir`, not `barezpakhsh.ir`. @vhidvz
- An import with a since-date no longer fails on the ledger's carry-forward row (missing `TransNumber`). @vhidvz

### Removed

- The Windows MSI package; the setup `.exe` installs per user and updates without administrator rights. @vhidvz
- The landau-parts name map: its Settings card, its table and the `calibrate` tool. @vhidvz
- `docs/` (specs, plans, runbook, calibration record, font note). @vhidvz

[unreleased]: https://github.com/vhidvz/landau-bridge/compare/v1.1.1...HEAD
[1.1.1]: https://github.com/vhidvz/landau-bridge/compare/v1.1.0...v1.1.1
[1.1.0]: https://github.com/vhidvz/landau-bridge/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/vhidvz/landau-bridge/releases/tag/v1.0.0
