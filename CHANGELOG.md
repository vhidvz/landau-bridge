# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Invoice pages are fetched in parallel, one per supplier session, 2 to 8 sessions as the link allows. @vhidvz
- Matches lists the parts a run left undecided, with review's saved candidates, to decide at any time. @vhidvz
- Decisions export to a JSON file and import on another install; your picks win, gone products are skipped. @vhidvz
- Long lists are paged: matches and undecided parts 50 rows a page, a review 20 parts a page. @vhidvz

### Changed

- Fewer sessions when added ones stop raising throughput (slow link) or 3 of 24 replies need a retry. @vhidvz
- Supplier requests retry six times over ~1¾ min, including a body cut off mid-download. @vhidvz
- Settings' login check keeps the short budget (three attempts in ~4 s) and answers in seconds. @vhidvz
- An invoice that fails is fetched once more at the end of the pass before it is reported. @vhidvz

### Fixed

- The site's error page (its `/Error` redirect answers 200) is retried, not reported as a layout change. @vhidvz
- An invoice the site keeps failing on is reported as `SupplierFaulted`, the site's fault, not its layout. @vhidvz
- README names the supplier host the app calls, `33314466.ir`, not `barezpakhsh.ir`. @vhidvz
- An import with a since-date no longer fails on the ledger's carry-forward row (missing `TransNumber`). @vhidvz

### Removed

- `docs/` (specs, plans, runbook, calibration record, font note); `calibrate` now writes `target/`. @vhidvz
