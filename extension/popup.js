// Chrono popup: current status, pause/resume, today's top domains (local, cheap).
'use strict';
const api = globalThis.browser || globalThis.chrome;
const $ = (id) => document.getElementById(id);

const WHY = {
  tracking: null,
  paused: 'Paused',
  'chrono-paused': 'Paused in Chrono',
  idle: 'Idle — not counting',
  locked: 'Screen locked',
  unfocused: 'Browser not in front',
  private: 'Private window — never tracked',
  'not-web': 'Not a web page',
  excluded: 'Excluded site',
  'no-tab': 'No tab',
  'no-window': 'No window',
  starting: 'Starting…',
};

function fmtDur(ms) {
  const m = Math.round(ms / 60000);
  if (m < 1) return '<1 min';
  if (m < 60) return `${m} min`;
  const h = Math.floor(m / 60);
  return `${h} h ${String(m % 60).padStart(2, '0')}`;
}
function fmtTime(ms) {
  return new Date(ms).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function render(s) {
  const now = s.now;
  const localPause = s.pausedUntil > now;
  const tracking = s.status === 'tracking' && s.cur;
  $('status').textContent = tracking ? s.cur.domain : WHY[s.status] || s.status;
  let detail = '';
  if (tracking) detail = `Tracking for ${fmtDur(now - s.cur.start)}`;
  else if (localPause) detail = s.pausedUntil > 8e15 ? 'Until you resume' : `Until ${fmtTime(s.pausedUntil)}`;
  else if (s.status === 'chrono-paused') detail = s.last && s.last.pausedUntil ? `Until ${fmtTime(s.last.pausedUntil)}` : 'Resume it in the Chrono app';
  $('detail').textContent = detail;

  $('pause-row').hidden = localPause;
  $('resume-row').hidden = !localPause;

  // Today: finished sessions + the live one.
  const totals = { ...(s.today && s.today.day ? s.today.totals : {}) };
  const today = new Date(now).toDateString();
  if (s.today && s.today.day) {
    const [y, m, d] = s.today.day.split('-').map(Number);
    if (new Date(y, m - 1, d).toDateString() !== today) for (const k of Object.keys(totals)) delete totals[k];
  }
  if (tracking) {
    const mid = new Date(now); mid.setHours(0, 0, 0, 0);
    totals[s.cur.domain] = (totals[s.cur.domain] || 0) + (now - Math.max(s.cur.start, mid.getTime()));
  }
  const top = Object.entries(totals).sort((a, b) => b[1] - a[1]).slice(0, 5);
  const max = top.length ? top[0][1] : 1;
  const ol = $('top');
  ol.replaceChildren(...top.map(([d, ms]) => {
    const li = document.createElement('li');
    const name = document.createElement('span'); name.className = 'name'; name.textContent = d;
    const time = document.createElement('span'); time.className = 'time'; time.textContent = fmtDur(ms);
    const bar = document.createElement('span'); bar.className = 'bar';
    const fill = document.createElement('i'); fill.style.transform = `scaleX(${Math.max(0.02, ms / max)})`;
    bar.append(fill); li.append(name, time, bar);
    return li;
  }));
  $('empty').hidden = top.length > 0;

  const agent = $('agent');
  const last = s.last;
  agent.className = 'muted';
  if (!last) agent.textContent = 'Connecting to the Selfkit agent…';
  else if (last.ok) agent.textContent = `Saved to Chrono · ${fmtTime(last.at)}`;
  else {
    agent.className = 'err';
    const n = s.queue.length;
    agent.textContent = `Selfkit agent not reachable${n ? ` · ${n} session${n === 1 ? '' : 's'} kept until it is` : ''}`;
  }
  const dot = $('dot');
  dot.className = 'dot' + (last && !last.ok ? ' err' : tracking ? ' on' : '');
}

async function refresh() {
  const s = await api.runtime.sendMessage({ type: 'state' });
  if (s) render(s);
}

for (const b of document.querySelectorAll('[data-min]')) {
  b.addEventListener('click', async () => {
    await api.runtime.sendMessage({ type: 'pause', minutes: Number(b.dataset.min) });
    refresh();
  });
}
$('resume').addEventListener('click', async () => {
  await api.runtime.sendMessage({ type: 'resume' });
  refresh();
});

refresh();
setInterval(refresh, 15000);
