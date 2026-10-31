#!/usr/bin/env node
/**
 * Make the built AppImage safe to ship: run after `tauri build --bundles appimage`.
 *
 *   node scripts/finalize-appimage.mjs [bundle dir]   # default: src-tauri/target/release/bundle/appimage
 *
 * **Never ship libwayland.** The AppImage is built on an old distribution so it runs on more of
 * them, and linuxdeploy copies that distribution's `libwayland-*` in. The GPU driver is never
 * bundled — EGL comes from the host's Mesa — and a current Mesa needs symbols an old libwayland
 * lacks (Wayland 1.23), so with the bundled copy loaded first WebKit cannot create its EGL display
 * and the window stays blank on Arch, Manjaro, Fedora 41+ (.memory/experiments-payment-pos-app/
 * appimage-wayland.md). Every host that can run this AppImage has its own libwayland (GTK 3 links
 * it) and it is backwards compatible, so the host's copy is always the right one.
 *
 * Patching linuxdeploy's GTK plugin no longer works: since Tauri CLI 2.12 the bundler embeds that
 * plugin and rewrites the cached copy whenever it differs. But it leaves the finished AppDir beside
 * the AppImage, so this removes the libraries there and packs the AppImage again with the same
 * linuxdeploy output plugin. Packing invalidates the updater signature the build wrote: with a
 * signing key in the environment the new file is signed again, bound to its version, without one
 * the stale `.sig` is removed — a signature that does not match is worse than none. `scripts/check-appimage.mjs`
 * gates the result.
 */
import { execFileSync } from 'node:child_process'
import { existsSync, readdirSync, rmSync } from 'node:fs'
import { homedir, platform } from 'node:os'
import { dirname, join, relative, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { conflictingLibraries } from './check-appimage.mjs'

function walk(dir, root = dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
    const p = join(dir, e.name)
    return e.isDirectory() ? walk(p, root) : [relative(root, p)]
  })
}

const only = (dir, suffix) => {
  const found = readdirSync(dir).filter((f) => f.endsWith(suffix))
  if (found.length !== 1) throw new Error(`expected one *${suffix} in ${dir}, found ${found.length}`)
  return found[0]
}

/** Strip, pack and (re)sign; `exec` and `env` are injected so the steps can be tested. */
export function finalize({ bundleDir, exec = execFileSync, env = process.env, toolsDir }) {
  const appDir = only(bundleDir, '.AppDir')
  const appImage = join(bundleDir, only(bundleDir, '.AppImage'))
  const removed = conflictingLibraries(walk(join(bundleDir, appDir)))
  if (!removed.length) return { removed, signed: false }
  for (const p of removed) rmSync(join(bundleDir, appDir, p))

  exec(join(toolsDir, 'linuxdeploy-plugin-appimage.AppImage'), ['--appdir', appDir], {
    cwd: bundleDir,
    env: { ...env, OUTPUT: appImage, ARCH: 'x86_64', APPIMAGE_EXTRACT_AND_RUN: '1' },
    stdio: 'inherit',
  })

  rmSync(`${appImage}.sig`, { force: true })
  const signed = Boolean(env.TAURI_SIGNING_PRIVATE_KEY || env.TAURI_SIGNING_PRIVATE_KEY_PATH)
  if (signed) {
    // Bound to the version as `tauri build` binds its own signatures, so the updater can refuse a
    // manifest that pairs a newer version number with an older package (requireSignedVersion).
    const version = /_([^_]+)_amd64\.AppImage$/.exec(appImage)?.[1]
    if (!version) throw new Error(`no version in ${appImage}`)
    exec('pnpm', ['exec', 'tauri', 'signer', 'sign', '--app-version', version, appImage], { env, stdio: 'inherit' })
  }
  return { removed, signed }
}

function main(args) {
  if (platform() !== 'linux') return 0
  const here = dirname(fileURLToPath(import.meta.url))
  const bundleDir = resolve(args[0] ?? join(here, '..', 'src-tauri/target/release/bundle/appimage'))
  const toolsDir = join(process.env.XDG_CACHE_HOME || join(homedir(), '.cache'), 'tauri')
  if (!existsSync(bundleDir)) {
    console.error(`finalize-appimage: no bundle directory at ${bundleDir}`)
    return 1
  }
  const { removed, signed } = finalize({ bundleDir, toolsDir })
  if (!removed.length) console.log('finalize-appimage: nothing to remove; the AppImage is left as built')
  else console.log(`finalize-appimage: removed ${removed.join(', ')}; packed again, ${signed ? 'signed' : 'unsigned (no key)'}`)
  return 0
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.exitCode = main(process.argv.slice(2))
}
