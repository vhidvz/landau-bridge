import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync, writeFileSync, existsSync, readdirSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { finalize } from './finalize-appimage.mjs'
import { conflictingLibraries } from './check-appimage.mjs'


test('libwayland anywhere under usr/lib is a conflict; everything else is not', () => {
  assert.deepEqual(
    conflictingLibraries([
      'usr/lib/libwayland-client.so.0',
      'usr/lib/x86_64-linux-gnu/libwayland-egl.so.1',
      'usr/lib/libwayland-server.so.0.22.0',
      'usr/lib/libgtk-3.so.0',
      'usr/lib/gtk-3.0/3.0.0/immodules/im-wayland.so',
      'usr/share/doc/libwayland-client0/copyright',
    ]),
    ['usr/lib/libwayland-client.so.0', 'usr/lib/x86_64-linux-gnu/libwayland-egl.so.1', 'usr/lib/libwayland-server.so.0.22.0'],
  )
})

/** A bundle directory as `tauri build --bundles appimage` leaves it. */
function bundle({ sig = true } = {}) {
  const dir = mkdtempSync(join(tmpdir(), 'finalize-'))
  const lib = join(dir, 'landau-bridge.AppDir/usr/lib')
  mkdirSync(join(lib, 'x86_64-linux-gnu'), { recursive: true })
  for (const f of ['libwayland-client.so.0', 'libwayland-egl.so.1', 'libgtk-3.so.0', 'x86_64-linux-gnu/libwayland-cursor.so.0'])
    writeFileSync(join(lib, f), 'so')
  writeFileSync(join(dir, 'landau-bridge_1.0.0_amd64.AppImage'), 'old image')
  if (sig) writeFileSync(join(dir, 'landau-bridge_1.0.0_amd64.AppImage.sig'), 'old sig')
  return dir
}

function recorder() {
  const calls = []
  return { calls, exec: (cmd, args, opts) => calls.push({ cmd, args, opts }) }
}

test('finalize strips libwayland from the AppDir and packs the AppImage again', () => {
  const dir = bundle()
  const { calls, exec } = recorder()
  const out = finalize({ bundleDir: dir, exec, env: {}, toolsDir: '/tools' })
  assert.deepEqual(out.removed.sort(), [
    'usr/lib/libwayland-client.so.0', 'usr/lib/libwayland-egl.so.1', 'usr/lib/x86_64-linux-gnu/libwayland-cursor.so.0'])
  assert.ok(existsSync(join(dir, 'landau-bridge.AppDir/usr/lib/libgtk-3.so.0')), 'everything else stays')
  assert.equal(calls[0].cmd, '/tools/linuxdeploy-plugin-appimage.AppImage')
  assert.deepEqual(calls[0].args, ['--appdir', 'landau-bridge.AppDir'])
  assert.equal(calls[0].opts.cwd, dir)
  assert.equal(calls[0].opts.env.OUTPUT, join(dir, 'landau-bridge_1.0.0_amd64.AppImage'))
  assert.equal(calls[0].opts.env.ARCH, 'x86_64')
})

test('with a signing key the packed AppImage is signed again', () => {
  const dir = bundle()
  const { calls, exec } = recorder()
  finalize({ bundleDir: dir, exec, env: { TAURI_SIGNING_PRIVATE_KEY: 'k' }, toolsDir: '/tools' })
  assert.equal(calls.length, 2)
  assert.deepEqual(calls[1].args.slice(-5), [
    'signer', 'sign', '--app-version', '1.0.0', join(dir, 'landau-bridge_1.0.0_amd64.AppImage')])
  assert.equal(existsSync(join(dir, 'landau-bridge_1.0.0_amd64.AppImage.sig')), false, 'the old signature never survives')
})

test('without a key the stale signature is removed, never left to fail verification', () => {
  const dir = bundle()
  const { calls, exec } = recorder()
  const out = finalize({ bundleDir: dir, exec, env: {}, toolsDir: '/tools' })
  assert.equal(calls.length, 1)
  assert.equal(out.signed, false)
  assert.equal(readdirSync(dir).some((f) => f.endsWith('.sig')), false)
})

test('an AppDir without libwayland is left as built', () => {
  const dir = mkdtempSync(join(tmpdir(), 'finalize-'))
  mkdirSync(join(dir, 'landau-bridge.AppDir/usr/lib'), { recursive: true })
  writeFileSync(join(dir, 'landau-bridge_1.0.0_amd64.AppImage'), 'image')
  const { calls, exec } = recorder()
  assert.deepEqual(finalize({ bundleDir: dir, exec, env: {}, toolsDir: '/tools' }).removed, [])
  assert.equal(calls.length, 0)
})
