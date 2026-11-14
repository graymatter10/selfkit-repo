// Logic tests for background.js with a fake WebExtension API: `bun test extension/test`.
import { expect, test } from 'bun:test';
const { createTracker, domainOf, excluded } = require('../background.js');

function fake() {
  const st = {};
  const env = {
    t: 1_763_200_000_000,
    win: { id: 1, focused: true, incognito: false },
    tab: { id: 7, url: 'https://www.example.com/a/b?q=1', incognito: false },
    native: [],
    reply: (msg) => ({ ok: true, paused: false, exclude_domains: [], agent_running: true, written: (msg.sessions || []).length }),
  };
  let n = 0;
  const api = {
    storage: { local: {
      get: async () => structuredClone(st),
      set: async (o) => { Object.assign(st, structuredClone(o)); },
    } },
    windows: { getLastFocused: async () => env.win },
    tabs: { query: async () => (env.tab ? [env.tab] : []) },
    runtime: { sendNativeMessage: async (host, msg) => {
      expect(host).toBe('com.selfkit.agent');
      env.native.push(structuredClone(msg));
      const r = env.reply(msg);
      if (r instanceof Error) throw r;
      return r;
    } },
    idle: { setDetectionInterval() {} },
  };
  const tr = createTracker(api, { now: () => env.t, uuid: () => `id-${++n}`, browser: 'chrome' });
  return { api, st, env, tr };
}

test('domainOf keeps the domain only', () => {
  expect(domainOf('https://www.Example.com:8443/path?x#y')).toBe('example.com');
  expect(domainOf('http://localhost:8000/')).toBe('localhost');
  expect(domainOf('chrome://settings')).toBe(null);
  expect(domainOf('file:///tmp/x.html')).toBe(null);
  expect(domainOf('about:blank')).toBe(null);
  expect(domainOf('')).toBe(null);
  expect(excluded('mail.bank.test', ['bank.test'])).toBe(true);
  expect(excluded('notbank.test', ['bank.test'])).toBe(false);
});

test('tab switches make sessions; non-web pages are not tracked', async () => {
  const { st, env, tr } = fake();
  await tr.evaluate('start');
  expect(st.cur.domain).toBe('example.com');
  env.t += 30_000;
  env.tab = { id: 8, url: 'https://news.test/x', incognito: false };
  await tr.evaluate('tab');
  expect(st.queue).toHaveLength(1);
  expect(st.queue[0]).toMatchObject({ id: 'id-1', domain: 'example.com', start: 1_763_200_000_000, end: 1_763_200_030_000 });
  expect(typeof st.queue[0].tz_offset).toBe('number');
  env.t += 10_000;
  env.tab = { id: 9, url: 'chrome://extensions', incognito: false };
  await tr.evaluate('tab');
  expect(st.cur).toBe(null);
  expect(st.status).toBe('not-web');
  expect(st.queue.map((q) => q.domain)).toEqual(['example.com', 'news.test']);
  expect(st.today.totals['example.com']).toBe(30_000);
});

test('private windows/tabs and unfocused windows are never tracked', async () => {
  const { st, env, tr } = fake();
  env.win = { id: 2, focused: true, incognito: true };
  await tr.evaluate('focus');
  expect(st.cur).toBe(null);
  expect(st.status).toBe('private');
  env.win = { id: 1, focused: true, incognito: false };
  env.tab = { id: 3, url: 'https://secret.test/', incognito: true };
  await tr.evaluate('tab');
  expect(st.cur).toBe(null);
  env.tab = { id: 4, url: 'https://ok.test/', incognito: false };
  env.win = { id: 1, focused: false, incognito: false };
  await tr.evaluate('focus');
  expect(st.status).toBe('unfocused');
  expect(st.queue).toHaveLength(0);
});

test('idle back-dates the end; suspend gap closes at the last heartbeat', async () => {
  const { st, env, tr } = fake();
  await tr.evaluate('start');
  for (let i = 0; i < 5; i++) { env.t += 60_000; await tr.evaluate('alarm'); }
  await tr.evaluate('idle', { idleState: 'idle' });
  expect(st.cur).toBe(null);
  expect(st.queue[0].end - st.queue[0].start).toBe(300_000 - 120_000);
  await tr.evaluate('idle', { idleState: 'active' });
  const startB = env.t;
  env.t += 60_000; await tr.evaluate('alarm');
  env.t += 3_600_000; // laptop lid closed
  await tr.evaluate('alarm');
  expect(st.queue[1]).toMatchObject({ start: startB, end: startB + 60_000 });
  expect(st.cur.start).toBe(env.t);
});

test('flush sends to the agent, buffers when unreachable, honours Chrono pause and excludes', async () => {
  const { st, env, tr } = fake();
  await tr.evaluate('start');
  env.t += 20_000;
  env.tab = { id: 8, url: 'https://b.test/', incognito: false };
  await tr.evaluate('tab');
  env.t += 5_000;
  env.reply = () => new Error('Specified native messaging host not found.');
  let r = await tr.flush();
  expect(r.ok).toBe(false);
  expect(st.queue).toHaveLength(1);
  expect(st.last.ok).toBe(false);
  env.reply = (msg) => ({ ok: true, paused: false, exclude_domains: ['b.test'], agent_running: true, written: msg.sessions.length });
  r = await tr.flush();
  expect(r.ok).toBe(true);
  const sent = env.native.at(-1);
  expect(sent.browser).toBe('chrome');
  expect(sent.sessions.map((s) => [s.domain, s.end - s.start])).toEqual([['example.com', 20_000], ['b.test', 5_000]]);
  expect(Object.keys(sent.sessions[0]).sort()).toEqual(['domain', 'end', 'id', 'start', 'tz_offset']);
  expect(st.queue).toHaveLength(0);
  // b.test is now excluded: the live session is closed and nothing new starts
  expect(st.cur).toBe(null);
  expect(st.status).toBe('excluded');
  env.tab = { id: 9, url: 'https://c.test/', incognito: false };
  env.reply = () => ({ ok: true, paused: true, paused_until: env.t + 600_000, exclude_domains: [] });
  await tr.flush();
  expect(st.status).toBe('chrono-paused');
  expect(st.cur).toBe(null);
});

test('popup pause and resume', async () => {
  const { st, env, tr } = fake();
  await tr.evaluate('start');
  env.t += 10_000;
  await tr.onMessage({ type: 'pause', minutes: 15 });
  expect(st.cur).toBe(null);
  expect(st.queue).toHaveLength(1);
  env.t += 60_000;
  await tr.evaluate('alarm');
  expect(st.status).toBe('paused');
  env.t += 15 * 60_000;
  await tr.evaluate('alarm');
  expect(st.status).toBe('tracking');
  await tr.onMessage({ type: 'pause', minutes: 0 });
  env.t += 86_400_000;
  await tr.evaluate('alarm');
  expect(st.status).toBe('paused');
  await tr.onMessage({ type: 'resume' });
  expect(st.status).toBe('tracking');
});

test('a session closed while its live copy was in flight is not lost', async () => {
  const { st, env, tr } = fake();
  await tr.evaluate('start');
  env.t += 10_000;
  await tr.evaluate('alarm');
  env.reply = (msg) => {
    // While the agent is busy, the user switches tab (the session closes later).
    env.t += 5_000;
    env.tab = { id: 8, url: 'https://b.test/', incognito: false };
    tr.evaluate('tab');
    return { ok: true, paused: false, exclude_domains: [], written: msg.sessions.length };
  };
  await tr.flush();
  expect(st.queue.map((q) => q.domain)).toEqual(['example.com']);
  expect(st.queue[0].end - st.queue[0].start).toBe(15_000);
});
