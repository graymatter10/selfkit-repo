// Chrono for Selfkit — background (Chrome service worker / Firefox background script).
// Records the domain of the active tab of the focused, non-private window while
// the user is not idle, as sessions {id, domain, start, end, tz_offset}, and sends
// them to the local Selfkit agent through native messaging (host
// "com.selfkit.agent"). Sessions are buffered in storage.local while the agent is
// unreachable. Domain only: no paths, no titles. Docs:
// docs/components/browser-extension.md
'use strict';

const HOST = 'com.selfkit.agent';
/** No heartbeat for this long = browser closed or machine asleep: end the session there. */
const GAP_MS = 150_000;
/** Shorter sessions are dropped. */
const MIN_MS = 1000;
/** Buffer cap while the agent is unreachable (oldest dropped first). */
const MAX_QUEUE = 5000;
const BATCH = 500;
const DEFAULT_IDLE_SECS = 120;

/** Domain we store for a URL: http(s) only, lowercase host, no port, no leading "www.". */
function domainOf(url) {
  if (!url) return null;
  let u;
  try {
    u = new URL(url);
  } catch {
    return null;
  }
  if (u.protocol !== 'http:' && u.protocol !== 'https:') return null;
  let h = u.hostname.toLowerCase().replace(/\.$/, '');
  if (h.startsWith('www.')) h = h.slice(4);
  return h || null;
}

/** An exclude entry matches the domain itself and its subdomains. */
function excluded(domain, list) {
  // Entries normalised like domains (www./port/scheme/case), so "www.youtube.com" or
  // "localhost:5173" typed in Chrono still match (C79).
  return (list || []).map(normEntry).some((e) => e && (domain === e || domain.endsWith('.' + e)));
}

function normEntry(e) {
  return String(e || '').trim().toLowerCase().replace(/^[a-z]+:\/\//, '').replace(/\/.*$/, '').replace(/:\d+$/, '').replace(/\.$/, '').replace(/^www\./, '');
}

function localDay(ms) {
  const d = new Date(ms);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

function localMidnight(ms) {
  const d = new Date(ms);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

const DEFAULT_STATE = {
  cur: null, // {id, domain, start, last}
  queue: [], // finished sessions not yet confirmed by the agent
  pausedUntil: 0, // local pause from the popup (ms; Infinity-like = 8.64e15)
  hostPaused: false, // Chrono / agent pause, as last reported by the agent
  blurred: false, // focus went to another program (WINDOW_ID_NONE), until a browser window is focused
  hostPausedUntil: 0, // its end (ms), 0 = open-ended; expires locally while the host is unreachable (L3-8)
  excludes: [], // Chrono exclude_domains, as last reported by the agent
  idleState: 'active',
  idleSecs: DEFAULT_IDLE_SECS,
  status: 'starting', // why we are (not) tracking right now
  today: { day: '', totals: {} }, // local per-domain ms for the popup
  last: null, // last native-messaging result {at, ok, error?, agent?, written?}
  ackedLive: null, // {id, end} of the open session as the agent last confirmed it
};

function createTracker(api, env = {}) {
  const now = env.now || (() => Date.now());
  const uuid = env.uuid || (() => crypto.randomUUID());
  const browserName = env.browser || (typeof globalThis.browser !== 'undefined' && api.runtime && api.runtime.getBrowserInfo ? 'firefox' : 'chrome');

  // Serialize every state change: events arrive concurrently.
  let chain = Promise.resolve();
  const serial = (fn) => {
    const p = chain.then(fn, fn);
    chain = p.catch(() => {});
    return p;
  };

  const load = async () => ({ ...DEFAULT_STATE, ...(await api.storage.local.get(null)) });
  const save = (s) => api.storage.local.set(s);

  function addToday(s, domain, start, end) {
    const day = localDay(end);
    if (s.today.day !== day) s.today = { day, totals: {} };
    const ms = end - Math.max(start, localMidnight(end));
    if (ms > 0) s.today.totals[domain] = (s.today.totals[domain] || 0) + ms;
  }

  function closeCur(s, end) {
    const c = s.cur;
    s.cur = null;
    if (!c) return;
    end = Math.max(c.start, Math.min(end, now()));
    if (end - c.start < MIN_MS) return;
    // Excluded sites (or time the host will drop) aren't shown in "Today in this browser" (L3-9).
    if (!excluded(c.domain, s.excludes || []) && !(s.hostPaused && !(s.hostPausedUntil && end >= s.hostPausedUntil))) addToday(s, c.domain, c.start, end);
    // The agent already has exactly this version (sent while it was live).
    if (s.ackedLive && s.ackedLive.id === c.id && s.ackedLive.end === end) return;
    s.queue.push({ id: c.id, domain: c.domain, start: c.start, end, tz_offset: -new Date(c.start).getTimezoneOffset() });
    if (s.queue.length > MAX_QUEUE) s.queue.splice(0, s.queue.length - MAX_QUEUE);
  }

  /** What should be tracked right now, and why (not). */
  async function target(s, t, opts = {}) {
    if (s.pausedUntil > t) return { domain: null, why: 'paused' };
    // A Chrono pause with an end expires here too, also while the host can't be reached (L3-8).
    if (s.hostPaused && !(s.hostPausedUntil && t >= s.hostPausedUntil)) return { domain: null, why: 'chrono-paused' };
    if (s.idleState !== 'active') return { domain: null, why: s.idleState };
    let win;
    try {
      win = await api.windows.getLastFocused({});
    } catch {
      return { domain: null, why: 'no-window' };
    }
    // The popup is its own window and takes focus: when it asks, ignore focus (L3-7).
    if (!win || (!win.focused && !opts.ignoreFocus)) return { domain: null, why: 'unfocused' };
    if (win.incognito) return { domain: null, why: 'private' };
    const tabs = await api.tabs.query({ active: true, windowId: win.id });
    const tab = tabs && tabs[0];
    if (!tab) return { domain: null, why: 'no-tab' };
    if (tab.incognito) return { domain: null, why: 'private' };
    const d = domainOf(tab.url || tab.pendingUrl);
    if (!d) return { domain: null, why: 'not-web' };
    if (excluded(d, s.excludes)) return { domain: null, why: 'excluded' };
    return { domain: d, why: 'tracking' };
  }

  /** Re-check the active tab/window/idle state and open/close sessions. */
  function evaluate(reason, opts = {}) {
    return serial(async () => {
      const s = await load();
      const t = now();
      if (opts.idleState) s.idleState = opts.idleState;
      // Focus left the browser (WINDOW_ID_NONE): stay "unfocused" until a browser window
      // gets focus again. On X11 getLastFocused() keeps saying focused, so the next tab
      // event or alarm reopened the session (Firefox: it never ended — C52/L3-6).
      if (opts.focusEvent) s.blurred = !!opts.blurred;
      if (s.cur && t - s.cur.last > GAP_MS) closeCur(s, s.cur.last);
      const want = s.blurred ? { domain: null, why: 'unfocused' } : await target(s, t);
      if (s.cur && s.cur.domain === want.domain) {
        s.cur.last = t;
      } else {
        if (s.cur) {
          // Going idle: the user left idleSecs ago.
          const end = opts.idleState && opts.idleState !== 'active' ? t - s.idleSecs * 1000 : t;
          closeCur(s, end);
        }
        if (want.domain) s.cur = { id: uuid(), domain: want.domain, start: t, last: t };
      }
      s.status = want.why;
      await save(s);
      updateBadge(s, t);
      return s;
    });
  }

  function updateBadge(s, t) {
    if (!api.action || !api.action.setBadgeText) return;
    const paused = s.pausedUntil > t || (s.hostPaused && !(s.hostPausedUntil && t >= s.hostPausedUntil));
    try {
      api.action.setBadgeText({ text: paused ? 'off' : '' });
      if (api.action.setBadgeBackgroundColor) api.action.setBadgeBackgroundColor({ color: '#6b7280' });
    } catch {
      /* not critical */
    }
  }

  /** Send finished sessions + the open one to the agent; keep them if it fails. */
  async function flush() {
    const s = await load();
    const live = s.cur && s.cur.last - s.cur.start >= MIN_MS
      ? [{ id: s.cur.id, domain: s.cur.domain, start: s.cur.start, end: s.cur.last, tz_offset: -new Date(s.cur.start).getTimezoneOffset() }]
      : [];
    const all = [...s.queue, ...live];
    const acked = new Map(); // id -> end that the agent confirmed
    let reply = null;
    let error = null;
    try {
      let i = 0;
      do {
        const batch = all.slice(i, i + BATCH);
        reply = await api.runtime.sendNativeMessage(HOST, batch.length ? { type: 'sessions', browser: browserName, sessions: batch } : { type: 'status' });
        if (!reply || !reply.ok) throw new Error((reply && reply.error) || 'no reply from agent');
        for (const x of batch) acked.set(x.id, x.end);
        i += BATCH;
      } while (i < all.length);
    } catch (e) {
      error = String((e && e.message) || e);
    }
    let pauseChanged = false;
    await serial(async () => {
      const s2 = await load();
      s2.queue = s2.queue.filter((q) => acked.get(q.id) !== q.end);
      if (live.length && acked.has(live[0].id)) s2.ackedLive = { id: live[0].id, end: live[0].end };
      if (reply && reply.ok) {
        pauseChanged = s2.hostPaused !== !!reply.paused;
        s2.hostPaused = !!reply.paused;
        s2.hostPausedUntil = reply.paused_until || 0;
        s2.excludes = Array.isArray(reply.exclude_domains) ? reply.exclude_domains : [];
      }
      s2.last = error
        ? { at: now(), ok: false, error, waiting: s2.queue.length }
        : { at: now(), ok: true, agent: !!reply.agent_running, written: reply.written || 0, pausedUntil: reply.paused_until || null };
      await save(s2);
    });
    // Chrono's "Time away" threshold (idle_minutes) applies to the browser too (L3-2).
    if (reply && reply.ok && Number.isFinite(reply.idle_minutes) && reply.idle_minutes > 0) {
      const secs = Math.round(reply.idle_minutes * 60);
      if (secs !== (await load()).idleSecs) await setIdleInterval(secs);
    }
    // A new Chrono pause or exclude list applies immediately.
    await evaluate(pauseChanged ? 'host-pause' : 'after-flush');
    return { ok: !error, error, sent: acked.size };
  }

  async function setIdleInterval(secs) {
    secs = Math.max(15, Math.round(secs || DEFAULT_IDLE_SECS));
    await serial(async () => {
      const s = await load();
      s.idleSecs = secs;
      await save(s);
    });
    if (api.idle && api.idle.setDetectionInterval) api.idle.setDetectionInterval(secs);
    return secs;
  }

  async function onMessage(msg) {
    switch (msg && msg.type) {
      case 'state': {
        // Look only: opening the popup took focus from the browser and used to close
        // (split) the session and show "Browser not in front" (L3-7).
        const s = await load();
        const t = now();
        const want = await target(s, t, { ignoreFocus: true });
        return { ...s, status: s.cur && s.cur.domain === want.domain ? 'tracking' : want.why, now: t, browser: browserName };
      }
      case 'pause': {
        const until = msg.minutes ? now() + msg.minutes * 60_000 : 8.64e15;
        await serial(async () => {
          const s = await load();
          s.pausedUntil = until;
          await save(s);
        });
        return evaluate('pause');
      }
      case 'resume':
        await serial(async () => {
          const s = await load();
          s.pausedUntil = 0;
          await save(s);
        });
        return evaluate('resume');
      case 'flush':
        return flush();
      case 'set-idle':
        return setIdleInterval(msg.secs);
      default:
        return null;
    }
  }

  /** Wire up browser events (called once per background start). */
  function install() {
    // 30 s (the minimum Chrome allows): a missed focus change costs at most half a minute (C52).
    api.alarms.create('tick', { periodInMinutes: 0.5 });
    api.alarms.onAlarm.addListener(async (a) => {
      if (a.name !== 'tick') return;
      await evaluate('alarm');
      await flush();
    });
    // Switching tabs means the user is in the browser: it also ends a "blurred" state (C52).
    api.tabs.onActivated.addListener(() => evaluate('tab', { focusEvent: true, blurred: false }));
    api.tabs.onUpdated.addListener((_id, change, tab) => {
      if (tab && tab.active && (change.url || change.status === 'complete')) evaluate('url');
    });
    api.tabs.onRemoved.addListener(() => evaluate('tab-closed'));
    // WINDOW_ID_NONE = focus went to another program: close the session now. On X11
    // getLastFocused().focused can stay true, so it ran until the next 1-min alarm (L3-1).
    api.windows.onFocusChanged.addListener((windowId) => evaluate('focus', { focusEvent: true, blurred: api.windows.WINDOW_ID_NONE !== undefined && windowId === api.windows.WINDOW_ID_NONE }));
    api.idle.onStateChanged.addListener((state) => evaluate('idle', { idleState: state }));
    api.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
      onMessage(msg).then(sendResponse, (e) => sendResponse({ error: String(e) }));
      return true; // async response
    });
    // Restore settings, then sync with the real idle state.
    load().then(async (s) => {
      if (api.idle.setDetectionInterval) api.idle.setDetectionInterval(s.idleSecs);
      const state = await api.idle.queryState(s.idleSecs);
      await evaluate('start', { idleState: state });
    });
  }

  return { evaluate, flush, onMessage, install, setIdleInterval, load };
}

if (typeof module !== 'undefined' && module.exports) {
  module.exports = { createTracker, domainOf, excluded, DEFAULT_STATE, GAP_MS };
} else {
  const api = globalThis.browser || globalThis.chrome;
  if (api && api.runtime && api.runtime.id) createTracker(api).install();
}
