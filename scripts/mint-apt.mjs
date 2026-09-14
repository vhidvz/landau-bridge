#!/usr/bin/env node
// Mint an apt-… key for a Landau fixture account and print the env exports the live tests need.
// Env: LND_API_URL (default http://localhost:6150), LND_ROLES_FILE (JSON array of {role,user,pass}),
//      LND_ROLE (default retail-supplier). Nothing here is ever written to disk.
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';

const API = process.env.LND_API_URL ?? 'http://localhost:6150';
const role = process.env.LND_ROLE ?? 'retail-supplier';
const roles = JSON.parse(readFileSync(process.env.LND_ROLES_FILE, 'utf8'));
const account = roles.find((r) => r.role === role);
if (!account) throw new Error(`no ${role} in ${process.env.LND_ROLES_FILE}`);

async function captcha() {
  const { algorithm, challenge, salt, signature, maxnumber = 1_000_000 } = await (await fetch(`${API}/challenge`)).json();
  for (let n = 0; n <= maxnumber; n++) {
    if (createHash('sha256').update(salt + n).digest('hex') === challenge) {
      return Buffer.from(JSON.stringify({ algorithm, challenge, number: n, salt, signature })).toString('base64');
    }
  }
  throw new Error('captcha unsolved');
}

const login = await fetch(`${API}/-/auth/token`, {
  method: 'POST',
  headers: { 'content-type': 'application/json' },
  body: JSON.stringify({ grant_type: 'password', response_type: 'token', username: account.user, password: account.pass, domain: 'landau.ir', captcha: await captcha() }),
});
const { access_token } = await login.json();
if (!access_token) throw new Error(`login failed (${login.status})`);

const minted = await fetch(`${API}/-/auth/apts?zone=own`, {
  method: 'POST',
  headers: { 'content-type': 'application/json', authorization: `Bearer ${access_token}` },
  body: JSON.stringify({ name: `landau-bridge ${new Date().toISOString().slice(0, 10)}`, expires_at: Date.now() + 90 * 86_400_000 }),
});
const body = await minted.json();
const token = body?.data?.token ?? body?.token;
if (!token) throw new Error(`mint failed (${minted.status}): ${JSON.stringify(body).slice(0, 200)}`);

// a fresh key answers 401 until the CDC watcher lands it; wait for it, then read the stores
let stores = [];
for (let i = 0; i < 30; i++) {
  const res = await fetch(`${API}/career/staff/memberships`, { headers: { authorization: `Bearer ${token}` } });
  if (res.status === 200) {
    const items = (await res.json()).items ?? [];
    stores = items.flatMap((m) => m.stores ?? []);
    break;
  }
  await new Promise((r) => setTimeout(r, 1000));
}
console.log(`export LND_API_URL=${API}`);
console.log(`export LND_APT_TOKEN=${token}`);
if (stores[0]) console.log(`export LND_STORE=${stores[0].id}   # ${stores.map((s) => `${s.id}=${s.name ?? ''}`).join(', ')}`);
