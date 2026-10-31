import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { bump, check, compareVersions, cutChangelog, isNewer, notes, parseVersion, releaseDay } from './release.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const FAKE = `node ${join(here, 'fixtures/fake-cargo.mjs')}`

const CHANGELOG = `# Changelog

## [Unreleased]

### Added

- A thing. @vhidvz
`

function fixture({ pkg = '0.1.0', cargo = '0.1.0', lock = '0.1.0', tauriVersion, changelog = CHANGELOG } = {}) {
  const root = mkdtempSync(join(tmpdir(), 'release-'))
  mkdirSync(join(root, 'src-tauri'))
  writeFileSync(join(root, 'package.json'), JSON.stringify({ name: 'landau-bridge', version: pkg }, null, 2) + '\n')
  writeFileSync(join(root, 'src-tauri/Cargo.toml'), `[package]\nname = "landau-bridge"\nversion = "${cargo}"\n\n[dependencies]\nserde = { version = "1" }\n`)
  writeFileSync(join(root, 'src-tauri/Cargo.lock'), `[[package]]\nname = "landau-bridge"\nversion = "${lock}"\n\n[[package]]\nname = "serde"\nversion = "1.0.0"\n`)
  const conf = { productName: 'landau-bridge', identifier: 'ir.landau.bridge' }
  if (tauriVersion) conf.version = tauriVersion
  writeFileSync(join(root, 'src-tauri/tauri.conf.json'), JSON.stringify(conf, null, 2) + '\n')
  writeFileSync(join(root, 'CHANGELOG.md'), changelog)
  return root
}
const read = (root, f) => readFileSync(join(root, f), 'utf8')
const opts = { allowDirty: true, cargo: FAKE, date: '2026-10-13' }

test('semver parsing and precedence', () => {
  assert.equal(parseVersion('1.0'), null)
  assert.equal(parseVersion('01.0.0'), null)
  assert.ok(parseVersion('1.0.1-test'))
  assert.ok(compareVersions('1.0.1-test', '1.0.1') < 0)
  assert.ok(compareVersions('1.0.0', '0.9.9') > 0)
  assert.equal(compareVersions('1.0.0', '1.0.0'), 0)
})

test('bump sets every mirror, drops the tauri.conf version and cuts the changelog', () => {
  const root = fixture({ tauriVersion: '0.1.0' })
  bump(root, '1.0.0', opts)
  assert.equal(JSON.parse(read(root, 'package.json')).version, '1.0.0')
  assert.match(read(root, 'src-tauri/Cargo.toml'), /\nversion = "1\.0\.0"\n/)
  assert.match(read(root, 'src-tauri/Cargo.toml'), /serde = \{ version = "1" \}/, 'dependency versions untouched')
  assert.match(read(root, 'src-tauri/Cargo.lock'), /name = "landau-bridge"\nversion = "1\.0\.0"/)
  assert.equal(JSON.parse(read(root, 'src-tauri/tauri.conf.json')).version, undefined)
  assert.match(read(root, 'CHANGELOG.md'), /## \[Unreleased\]\n\n## \[1\.0\.0\] - 2026-10-13\n\n### Added\n\n- A thing\. @vhidvz\n/)
  assert.deepEqual(check(root), [])
})

test('a re-run with the same version changes nothing and never duplicates the section', () => {
  const root = fixture()
  bump(root, '1.0.0', opts)
  const before = read(root, 'CHANGELOG.md')
  bump(root, '1.0.0', opts)
  assert.equal(read(root, 'CHANGELOG.md'), before)
})

test('a failing cargo step restores all four files byte for byte', () => {
  const root = fixture()
  const files = ['package.json', 'src-tauri/Cargo.toml', 'src-tauri/Cargo.lock', 'CHANGELOG.md']
  const before = files.map((f) => read(root, f))
  process.env.FAKE_CARGO_FAIL = '1'
  try {
    assert.throws(() => bump(root, '1.0.0', opts), /cargo/)
  } finally {
    delete process.env.FAKE_CARGO_FAIL
  }
  assert.deepEqual(files.map((f) => read(root, f)), before)
})

test('a lower version and an empty Unreleased section are refused', () => {
  assert.throws(() => bump(fixture({ cargo: '1.2.0', pkg: '1.2.0', lock: '1.2.0' }), '1.1.0', opts), /lower/)
  const empty = fixture({ changelog: '# Changelog\n\n## [Unreleased]\n' })
  assert.throws(() => bump(empty, '1.0.0', opts), /empty/)
  bump(empty, '1.0.0', { ...opts, allowEmpty: true })
})

test('check names every mismatch', () => {
  const root = fixture({ pkg: '1.0.0', lock: '0.0.9', tauriVersion: '0.1.0' })
  const problems = check(root)
  assert.equal(problems.length, 3, problems.join('\n'))
})

test('check with a tag wants v<version> and a non-empty section', () => {
  const root = fixture()
  bump(root, '1.0.0', opts)
  assert.deepEqual(check(root, 'v1.0.0'), [])
  assert.equal(check(root, 'v1.0.1').length, 1)
  assert.equal(check(root, 'refs/tags/v1.0.0').length, 0, 'a full ref is accepted')
})

test('notes extracts exactly one section', () => {
  const text = cutChangelog(CHANGELOG, '1.0.0', '2026-10-13', {})
  assert.equal(notes(text, '1.0.0'), '### Added\n\n- A thing. @vhidvz')
  assert.throws(() => notes(text, '2.0.0'), /no section/)
})

test('check-newer refuses an equal or lower version and accepts the first release', () => {
  assert.equal(isNewer('1.0.0', 'v1.0.0'), false)
  assert.equal(isNewer('1.0.0', 'v1.0.1'), false)
  assert.equal(isNewer('1.0.0', 'v0.9.0'), true)
  assert.equal(isNewer('1.0.1-rc.1', 'v1.0.0'), true)
  assert.equal(isNewer('1.0.0', ''), true, 'nothing published yet')
})

test("the release day is Tehran's, not UTC's", () => {
  assert.equal(releaseDay(new Date('2026-10-30T21:00:00Z')), '2026-10-31', '00:30 in Tehran')
  assert.equal(releaseDay(new Date('2026-10-30T20:00:00Z')), '2026-10-30', '23:30 in Tehran')
})

test('bump leaves a tauri.conf without a version byte for byte', () => {
  const root = fixture()
  const compact = '{ "productName": "landau-bridge", "identifier": "ir.landau.bridge", "bundle": { "targets": ["deb"] } }\n'
  writeFileSync(join(root, 'src-tauri/tauri.conf.json'), compact)
  bump(root, '1.0.0', opts)
  assert.equal(read(root, 'src-tauri/tauri.conf.json'), compact)
})
