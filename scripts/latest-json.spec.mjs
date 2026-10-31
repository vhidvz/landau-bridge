import { test } from 'node:test'
import assert from 'node:assert/strict'
import { buildManifest, sums } from './latest-json.mjs'

const V = '1.0.0'
const BASE = 'https://github.com/vhidvz/landau-bridge/releases/download/v1.0.0'
const BUNDLES = [
  'landau-bridge_1.0.0_amd64.AppImage',
  'landau-bridge_1.0.0_amd64.deb',
  'landau-bridge-1.0.0-1.x86_64.rpm',
  'landau-bridge_1.0.0_x64-setup.exe',
]
const ALL = BUNDLES.flatMap((f) => [f, `${f}.sig`])
const read = (f) => `sig of ${f.replace(/\.sig$/, '')}`
const opts = { version: V, notes: 'Notes.', pubDate: '2026-10-31T10:00:00Z', base: BASE }

test('every key points at its file and signature', () => {
  const m = buildManifest(ALL, read, opts)
  assert.equal(m.version, V)
  assert.equal(m.notes, 'Notes.')
  assert.equal(m.pub_date, '2026-10-31T10:00:00Z')
  const entry = (f) => ({ url: `${BASE}/${f}`, signature: `sig of ${f}` })
  assert.deepEqual(m.platforms, {
    'linux-x86_64': entry(BUNDLES[0]),
    'linux-x86_64-appimage': entry(BUNDLES[0]),
    'linux-x86_64-deb': entry(BUNDLES[1]),
    'linux-x86_64-rpm': entry(BUNDLES[2]),
    'windows-x86_64': entry(BUNDLES[3]),
    'windows-x86_64-nsis': entry(BUNDLES[3]),
  })
})

test('a missing signature fails, naming the file', () => {
  const files = ALL.filter((f) => f !== 'landau-bridge_1.0.0_amd64.deb.sig')
  assert.throws(() => buildManifest(files, read, opts), /landau-bridge_1\.0\.0_amd64\.deb\.sig/)
})

test('a missing platform fails', () => {
  const files = ALL.filter((f) => !f.includes('setup.exe'))
  assert.throws(() => buildManifest(files, read, opts), /windows-x86_64-nsis/)
})

test('two files for one key fail', () => {
  const files = [...ALL, 'old-name_1.0.0_amd64.deb', 'old-name_1.0.0_amd64.deb.sig']
  assert.throws(() => buildManifest(files, read, opts), /linux-x86_64-deb/)
})

test('a file named for another version fails', () => {
  const files = ALL.map((f) => f.replace('1.0.0', '1.0.1'))
  assert.throws(() => buildManifest(files, read, opts), /1\.0\.1|version/)
})

test('sums list every bundle, not the signatures', () => {
  const out = sums(ALL, (f) => Buffer.from(f))
  const lines = out.trim().split('\n')
  assert.equal(lines.length, 4)
  assert.match(lines[0], /^[0-9a-f]{64}  landau-bridge/)
  assert.ok(!out.includes('.sig'))
})

test('a prerelease resolves every key under the bundler\'s own file names', () => {
  const rc = [
    'landau-bridge_1.0.1-rc.1_amd64.AppImage',
    'landau-bridge_1.0.1-rc.1_amd64.deb',
    'landau-bridge-1.0.1-rc.1-1.x86_64.rpm',
    'landau-bridge_1.0.1-rc.1_x64-setup.exe',
  ].flatMap((f) => [f, `${f}.sig`])
  const m = buildManifest(rc, read, { ...opts, version: '1.0.1-rc.1' })
  assert.equal(Object.keys(m.platforms).length, 6)
  assert.match(m.platforms['linux-x86_64-rpm'].url, /landau-bridge-1\.0\.1-rc\.1-1\.x86_64\.rpm$/)
})
