# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- README names the supplier host the app calls, `33314466.ir`, not `barezpakhsh.ir`. @vhidvz
- An import with a since-date no longer fails on the ledger's carry-forward row (missing `TransNumber`). @vhidvz

### Removed

- `docs/` (specs, plans, runbook, calibration record, font note); `calibrate` now writes `target/`. @vhidvz
