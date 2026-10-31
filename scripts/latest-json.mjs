#!/usr/bin/env node
/**
 * Write the updater manifest and the checksums for one release, once, from every built bundle.
 *
 *   node scripts/latest-json.mjs --dir <bundles> --version <x.y.z> --notes-file <md> --base <url>
 *
 * `--base` is the release's download URL (`…/releases/download/v<x.y.z>`). Writes
 * `<dir>/latest.json` and `<dir>/SHA256SUMS`. The updater looks up `{os}-{arch}-{installer}`, then
 * `{os}-{arch}` (tauri-plugin-updater `get_urls`), so each installer gets its own key and the plain
 * key goes to the AppImage and the NSIS setup. Built in one place after every platform's build —
 * never per build job, where parallel jobs would overwrite each other's keys — and it refuses an
 * incomplete set, so `releases/latest` never serves a manifest a platform cannot update from.
 */
import { createHash } from 'node:crypto'
import { readdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

/** key → the bundle it serves, by file name. */
const KEYS = [
  [['linux-x86_64-appimage', 'linux-x86_64'], (v) => new RegExp(`_${esc(v)}_amd64\\.AppImage$`)],
  [['linux-x86_64-deb'], (v) => new RegExp(`_${esc(v)}_amd64\\.deb$`)],
  [['linux-x86_64-rpm'], (v) => new RegExp(`-${esc(v)}-1\\.x86_64\\.rpm$`)],
  [['windows-x86_64-nsis', 'windows-x86_64'], (v) => new RegExp(`_${esc(v)}_x64-setup\\.exe$`)],
]
const BUNDLE = /\.(AppImage|deb|rpm|exe)$/
const esc = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')

/** The manifest for `files` (names in one directory); `read(name)` gives a `.sig`'s contents. */
export function buildManifest(files, read, { version, notes, pubDate, base }) {
  const bundles = files.filter((f) => BUNDLE.test(f))
  const platforms = {}
  for (const [keys, pattern] of KEYS) {
    const re = pattern(version)
    const hits = bundles.filter((f) => re.test(f))
    if (hits.length !== 1) {
      throw new Error(`${keys[0]}: expected one bundle for version ${version}, found ${hits.length} (${hits.join(', ') || 'none'})`)
    }
    const [file] = hits
    if (!files.includes(`${file}.sig`)) throw new Error(`${keys[0]}: ${file}.sig is missing`)
    const entry = { signature: read(`${file}.sig`).trim(), url: `${base}/${file}` }
    for (const k of keys) platforms[k] = entry
  }
  return { version, notes, pub_date: pubDate, platforms }
}

/** `sha256sum` lines for every bundle; `readBytes(name)` gives its contents. */
export function sums(files, readBytes) {
  return files
    .filter((f) => BUNDLE.test(f))
    .sort()
    .map((f) => `${createHash('sha256').update(readBytes(f)).digest('hex')}  ${f}\n`)
    .join('')
}

function main(argv) {
  const value = (name) => (argv.includes(name) ? argv[argv.indexOf(name) + 1] : undefined)
  const dir = value('--dir')
  const version = value('--version')
  const notesFile = value('--notes-file')
  const base = value('--base')
  if (!dir || !version || !notesFile || !base) {
    console.error('usage: latest-json.mjs --dir <bundles> --version <x.y.z> --notes-file <md> --base <url>')
    return 2
  }
  const files = readdirSync(dir)
  const manifest = buildManifest(files, (f) => readFileSync(join(dir, f), 'utf8'), {
    version,
    notes: readFileSync(notesFile, 'utf8').trim(),
    pubDate: new Date().toISOString(),
    base: base.replace(/\/$/, ''),
  })
  writeFileSync(join(dir, 'latest.json'), JSON.stringify(manifest, null, 2) + '\n')
  writeFileSync(join(dir, 'SHA256SUMS'), sums(files, (f) => readFileSync(join(dir, f))))
  console.log(`latest-json: ${Object.keys(manifest.platforms).length} keys for ${version} in ${dir}`)
  return 0
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    process.exitCode = main(process.argv.slice(2))
  } catch (e) {
    console.error(`latest-json: ${e.message}`)
    process.exitCode = 1
  }
}
