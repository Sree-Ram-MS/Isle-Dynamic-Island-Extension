// Isle for Windows — renderer, matched frame-by-frame to the Nusa app
// in Ram's reference video: a compact notch that hugs its content, a
// status strip (page name · date · time) on every page, side chevrons,
// and dense two-column page layouts. Music comes from the real Windows
// media sessions via the main process — never faked.
const $ = (s) => document.querySelector(s);

const PAGES = [
    {id: 'music', icon: '🎵', name: 'Music'},
    {id: 'pomodoro', icon: '🎯', name: 'Pomodoro'},
    {id: 'water', icon: '💧', name: 'Water'},
    {id: 'shelf', icon: '🗂️', name: 'Shelf'},
    {id: 'todo', icon: '✔️', name: 'TodoList'},
    {id: 'calendar', icon: '📅', name: 'Calendar'},
];
const TODO_COLORS = ['#ffb454', '#ff5a52', '#7ed492', '#57a7ff', '#c792ff'];

let S = null;
let data = {todos: [], shelf: [], waterlog: {day: '', drinks: []}};
let expanded = false;
let current = 'music';
let popup = null;
let battery = null;
let dueTask = null;
let lastWaterReminder = '';

const save = (key) => window.isle.storeSet(key, data[key]);
const visiblePages = () => PAGES.filter(p => !S || S.pages[p.id] !== false);
const esc = (s) => String(s ?? '').replace(/[&<>"]/g, c => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;'}[c]));
const fmtClock = (sec) => {
    if (!isFinite(sec) || sec < 0) sec = 0;
    return `${Math.floor(sec / 60)}:${String(Math.floor(sec % 60)).padStart(2, '0')}`;
};
const todayKey = () => new Date().toDateString();

// ---------------- sizing: the notch hugs its content ----------------
const PILL_SIZES = {line: [192, 12], clock: [192, 40], eyes: [140, 40],
    popup: [320, 40], pomo: [250, 40], music: [330, 40]};
const BOLT_SVG = '<span class="bolt"><svg viewBox="0 0 24 24"><path d="M13.2 1.6 5 13.4h5.4L9 22.4l8.2-11.8h-5.4l1.4-9z"/></svg></span>';
const LYR_SVG = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M21 11.5a8.5 8.5 0 0 1-8.5 8.5c-1.4 0-2.8-.34-4-.97L3 20l1.03-5.5A8.5 8.5 0 1 1 21 11.5z"/><path d="M8.6 9.2v3M12.4 9.2v3"/></svg>';
let lastSentSize = '';
function sendSize(w, h) {
    const key = `${w}x${h}`;
    if (key === lastSentSize) return;
    lastSentSize = key;
    window.isle.setSize(w, h);
}
function fit() {
    if (!expanded) {
        const s = PILL_SIZES[pillKind] || PILL_SIZES.clock;
        sendSize(s[0], s[1]);
        return;
    }
    if (current === 'music') sendSize(...(music.lyricsOn ? [664, 208] : [544, 224]));
    else if (current === 'pomodoro') sendSize(544, 198);
    else if (current === 'water') sendSize(544, 198);
    else if (current === 'calendar') sendSize(544, 314);
    else if (current === 'shelf') {
        const n = data.shelf.length;
        sendSize(544, n === 0 ? 198 : Math.min(100 + n * 43, 332));
    } else if (current === 'todo') {
        const n = Math.max(data.todos.length, 1);
        sendSize(544, Math.min(100 + Math.max(118, n * 43) + (dueTask ? 48 : 0), 332));
    }
}

// ---------------- popups ----------------
function showPopup(icon, text, ms = 4000) {
    popup = {icon, text, until: Date.now() + ms};
    renderPill(true);
}

// ---------------- music ----------------
const music = {
    info: null, key: null, pos: 0, stamp: 0,
    lines: [], lyricsState: 'idle', lyricsOn: false, activeLine: -1, els: null,
};
function curPos() {
    if (!music.info) return 0;
    if (!music.info.playing) return music.pos;
    return Math.min(music.info.duration || 1e9, music.pos + (Date.now() - music.stamp) / 1000);
}
async function refreshMusic() {
    let info = null;
    try { info = await window.isle.musicInfo(); } catch { /* bridge down */ }
    const had = music.key;
    if (!info || !info.hasSession) {
        if (music.key !== null) {
            music.info = null; music.key = null; music.lines = []; music.lyricsState = 'idle';
            renderPill(true);
            if (expanded && current === 'music') showPage('music');
        }
        return;
    }
    const key = `${info.title}|${info.artist}`;
    music.info = info;
    music.pos = info.position || 0;
    music.stamp = Date.now();
    if (key !== had) {
        music.key = key;
        music.lines = []; music.lyricsState = 'loading'; music.activeLine = -1;
        fetchLyrics(info);
        renderPill(true);
        if (expanded && current === 'music') showPage('music');
    }
}
async function fetchLyrics(info) {
    const key = music.key;
    try {
        // The original queries LRCLIB's /api/search with track + artist and
        // picks the synced result closest in duration (extracted from the
        // app's own strings: "lrclib.net/api/search?track_name=&artist_name=").
        const q = new URLSearchParams({
            track_name: info.title || '', artist_name: info.artist || '',
        });
        const r = await fetch(`https://lrclib.net/api/search?${q}`);
        if (!r.ok) throw new Error('no match');
        const results = await r.json();
        if (music.key !== key) return;
        const dur = info.duration || 0;
        const candidates = (Array.isArray(results) ? results : []).filter(x => x && x.syncedLyrics);
        candidates.sort((a, b) => Math.abs((a.duration || 0) - dur) - Math.abs((b.duration || 0) - dur));
        const pick = candidates[0];
        if (!pick) { music.lyricsState = 'none'; }
        else {
            music.lines = pick.syncedLyrics.split('\n').map(line => {
                const m = line.match(/\[(\d+):(\d+(?:\.\d+)?)\](.*)/);
                return m ? {t: +m[1] * 60 + +m[2], text: m[3].trim()} : null;
            }).filter(l => l && l.text);
            music.lyricsState = music.lines.length ? 'ok' : 'none';
        }
    } catch {
        if (music.key === key) music.lyricsState = 'none';
    }
    if (expanded && current === 'music') showPage('music');
}
function activeLineIdx() {
    const p = curPos();
    let idx = -1;
    for (let i = 0; i < music.lines.length; i++) {
        if (music.lines[i].t <= p) idx = i; else break;
    }
    return idx;
}
function paneLinesHtml() {
    if (music.lyricsState === 'loading') return '<div class="none">Loading lyrics</div>';
    if (music.lyricsState !== 'ok') return '<div class="none">Lyrics not available</div>';
    const idx = activeLineIdx();
    let html = '';
    for (let i = idx - 2; i <= idx + 2; i++) {
        html += i >= 0 && i < music.lines.length
            ? `<div class="ln${i === idx ? ' cur' : ''}" data-i="${i}">${esc(music.lines[i].text)}</div>`
            : '<div class="ln">&nbsp;</div>';
    }
    return html;
}

// ---------------- pomodoro ----------------
const pomo = {label: 'Focus', dur: 25 * 60, left: 25 * 60, running: false, endAt: 0, draw: null};
function chime() {
    if (!S || !S.sounds) return;
    try {
        const ctx = chime.ctx || (chime.ctx = new AudioContext());
        [880, 659].forEach((f, i) => {
            const o = ctx.createOscillator(), g = ctx.createGain();
            o.frequency.value = f;
            o.type = 'sine';
            g.gain.setValueAtTime(0.0001, ctx.currentTime + i * 0.18);
            g.gain.exponentialRampToValueAtTime(0.12, ctx.currentTime + i * 0.18 + 0.03);
            g.gain.exponentialRampToValueAtTime(0.0001, ctx.currentTime + i * 0.18 + 0.5);
            o.connect(g).connect(ctx.destination);
            o.start(ctx.currentTime + i * 0.18);
            o.stop(ctx.currentTime + i * 0.18 + 0.55);
        });
    } catch { /* no audio device */ }
}

// ---------------- idle / mascot ----------------
const idle = {last: Date.now(), waved: false};
function poke() { idle.last = Date.now(); idle.waved = false; }
document.addEventListener('mousemove', () => {
    if (Date.now() - idle.last > 2000) poke();
}, {passive: true});
document.addEventListener('keydown', poke);

// ---------------- the collapsed pill ----------------
let pillKind = '', pillSig = '';
function renderPill(force) {
    const now = Date.now();
    let kind, html, sig;
    if (popup && now < popup.until) {
        kind = 'popup';
        sig = popup.text;
        html = `<span style="font-size:15px">${popup.icon}</span><span class="popTx">${esc(popup.text)}</span>`;
    } else if (pomo.running) {
        kind = 'pomo';
        sig = `p${Math.ceil(pomo.left)}`;
        html = `<span style="font-size:14px">🎯</span><span class="clock">${fmtClock(pomo.left)}</span><span style="font-size:11.5px;color:#a9a9b3">${esc(pomo.label)}</span>`;
    } else if (music.info && music.info.hasSession) {
        kind = 'music';
        const p = curPos(), d = music.info.duration || 0;
        sig = `m${music.key}|${Math.floor(p)}`;
        html = `${music.info.art
            ? `<img class="miniArt" src="data:${music.info.artType || 'image/jpeg'};base64,${music.info.art}">`
            : '<span class="miniArtPh">♪</span>'}
          <span class="miniTx"><div class="miniTitle">${esc(music.info.title) || 'Unknown title'}</div>
          <div class="miniSub">${esc(music.info.artist)}${d ? ` · ${fmtClock(p)} · -${fmtClock(Math.max(0, d - p))}` : ''}</div></span>
          <span style="font-size:12px">${music.info.playing ? '▶' : '⏸'}</span>`;
    } else if (S && S.mascot && !expanded && now - idle.last > 25000) {
        kind = 'eyes';
        const dozing = now - idle.last > 85000;
        sig = `e${dozing}`;
        html = `<span class="eyes${dozing ? ' doze' : ''}" id="eyesEl"><span class="eye"></span><span class="eye"></span>${dozing ? '<span class="zzz">z z</span>' : ''}</span>`;
    } else if (S && S.minimal) {
        kind = 'line';
        sig = 'line';
        html = '';
    } else {
        kind = 'clock';
        const d = new Date();
        sig = `c${d.getMinutes()}${battery && battery.charging ? 'chg' : ''}`;
        html = `${battery && battery.charging ? BOLT_SVG : ''}
                <span class="clock">${d.toLocaleTimeString([], {hour: '2-digit', minute: '2-digit'})}</span>`;
    }
    pillKind = kind;
    const pill = $('#pill');
    pill.style.display = kind === 'line' ? 'none' : '';
    $('#line').hidden = kind !== 'line';
    if (sig !== pillSig || force) {
        pillSig = sig;
        if (kind !== 'line') pill.innerHTML = html;
        pill.classList.toggle('idle', kind === 'clock' && now - idle.last > 10000);
    }
    fit();
}

// ---------------- strip (shown on every page) ----------------
function tickStrip() {
    const d = new Date();
    const days = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
    const mons = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
    $('#stripDate').textContent = `${days[d.getDay()]} ${d.getDate()} ${mons[d.getMonth()]} ${d.getFullYear()}`;
    $('#stripTime').textContent = d.toLocaleTimeString([], {hour: '2-digit', minute: '2-digit'});
    $('#stripBolt').hidden = !(battery && battery.charging);
}
function setStripAction(buttons) {
    const slot = $('#stripAction');
    slot.innerHTML = '';
    for (const a of buttons || []) {
        const b = document.createElement('button');
        b.textContent = a.label;
        b.onclick = (e) => a.fn(e);
        slot.appendChild(b);
    }
}

// ---------------- expand / collapse / page switching ----------------
let openT = null, closeT = null;
function openCard(page) {
    if (page) current = page;
    if (!visiblePages().some(p => p.id === current)) current = visiblePages()[0]?.id || 'music';
    expanded = true;
    $('#pill').style.display = 'none';
    $('#line').hidden = true;
    $('#card').hidden = false;
    showPage(current);
}
function closeCard() {
    expanded = false;
    $('#card').hidden = true;
    $('#pill').style.display = '';
    pillSig = '';
    renderPill(true);
}
$('#notch').addEventListener('mouseenter', () => {
    clearTimeout(closeT);
    poke();
    if (pillKind === 'eyes' && !idle.waved) {
        idle.waved = true;
        const e = $('#eyesEl');
        if (e) e.classList.add('wave');
        setTimeout(() => { poke(); renderPill(true); }, 720);
    }
    if (!expanded) openT = setTimeout(() => openCard(), 140);
});
document.documentElement.addEventListener('mouseleave', () => {
    clearTimeout(openT);
    if (expanded) closeT = setTimeout(closeCard, 650);
});
$('#bellBtn').addEventListener('click', () => {
    if (expanded) showPage('todo'); else openCard('todo');
});
function updateBell() {
    const now = new Date();
    const mins = now.getHours() * 60 + now.getMinutes();
    const hot = !!dueTask || data.todos.some(t => {
        if (t.done || !t.time) return false;
        const [h, m] = t.time.split(':').map(Number);
        const dm = h * 60 + m;
        return dm <= mins + 30 && dm >= mins - 120;
    });
    $('#bellDot').hidden = !hot;
}
function stepPage(dir) {
    const pages = visiblePages();
    let i = pages.findIndex(p => p.id === current);
    showPage(pages[(i + dir + pages.length) % pages.length].id);
}
$('#chevL').addEventListener('click', () => stepPage(-1));
$('#chevR').addEventListener('click', () => stepPage(1));
$('#card').addEventListener('wheel', (e) => {
    const now = Date.now();
    if (now - (stepPage._at || 0) < 350) return;
    stepPage._at = now;
    stepPage(e.deltaY > 0 ? 1 : -1);
}, {passive: true});
document.addEventListener('keydown', (e) => {
    if (!expanded || e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') return;
    if (e.key === 'ArrowRight') stepPage(1);
    if (e.key === 'ArrowLeft') stepPage(-1);
});

// ---------------- navigation dots + hover icon bar ----------------
function buildNav() {
    const dots = $('#dots'), bar = $('#iconbar');
    dots.innerHTML = '';
    bar.innerHTML = '';
    for (const p of visiblePages()) {
        const d = document.createElement('button');
        d.className = 'dot' + (p.id === current ? ' active' : '');
        d.title = p.name;
        d.addEventListener('click', () => showPage(p.id));
        dots.appendChild(d);
        const b = document.createElement('button');
        b.textContent = p.icon;
        b.title = p.name;
        b.className = p.id === current ? 'active' : '';
        b.addEventListener('click', () => showPage(p.id));
        bar.appendChild(b);
    }
    let hideT = null;
    const show = () => { clearTimeout(hideT); bar.classList.add('show'); };
    const hide = () => { hideT = setTimeout(() => bar.classList.remove('show'), 350); };
    dots.onmouseenter = show; dots.onmouseleave = hide;
    bar.onmouseenter = show; bar.onmouseleave = hide;
}
function showPage(id) {
    current = id;
    const split = id === 'music' && music.lyricsOn;
    $('#strip').style.display = split ? 'none' : '';
    // The original leaves the strip's left slot empty on the Music page.
    $('#stripTitle').textContent = id === 'music' ? '' : ((PAGES.find(p => p.id === id) || {}).name || '');
    setStripAction([]);
    const body = $('#pageBody');
    body.innerHTML = '';
    const el = document.createElement('div');
    el.className = 'page';
    body.appendChild(el);
    RENDER[id](el);
    buildNav();
    fit();
}

// ---------------- pages ----------------
const RENDER = {};

function musicControlsHtml(info) {
    return `
      <div class="mSeekRow">
        <span class="tm" id="mPos">0:00</span>
        <div class="seek" id="mSeek"><div id="mSeekFill"></div></div>
        <span class="tm" id="mDur">0:00</span>
      </div>
      <div class="mCtlRow">
        <span class="side"><button id="mLyr" class="lyrBtn${music.lyricsOn ? ' active' : ''}" title="Lyrics">${LYR_SVG}</button></span>
        <span class="sp">
          <button id="mPrev" title="Previous">⏮</button>
          <button id="mPlay" title="Play / pause">${info.playing ? '⏸' : '▶'}</button>
          <button id="mNext" title="Next">⏭</button>
        </span>
        <span class="side"><span class="volWrap"><span style="font-size:14px">🔊</span><input type="range" id="mVol" min="0" max="100"></span></span>
      </div>`;
}
function wireMusicControls(el) {
    const info = music.info;
    music.els = {
        seek: el.querySelector('#mSeek'), fill: el.querySelector('#mSeekFill'),
        pos: el.querySelector('#mPos'), dur: el.querySelector('#mDur'),
        play: el.querySelector('#mPlay'), pane: el.querySelector('#mPane'),
    };
    el.querySelector('#mPrev').onclick = () => window.isle.musicCmd('prev').then(() => setTimeout(refreshMusic, 450));
    el.querySelector('#mNext').onclick = () => window.isle.musicCmd('next').then(() => setTimeout(refreshMusic, 450));
    music.els.play.onclick = () => window.isle.musicCmd('playpause').then(() => setTimeout(refreshMusic, 350));
    el.querySelector('#mLyr').onclick = () => {
        music.lyricsOn = !music.lyricsOn;
        showPage('music');
    };
    music.els.seek.onclick = (e) => {
        const d = music.info.duration || 0;
        if (!d) return;
        const r = music.els.seek.getBoundingClientRect();
        const ratio = (e.clientX - r.left) / r.width;
        window.isle.musicCmd('seek', Math.round(ratio * d));
        music.pos = ratio * d;
        music.stamp = Date.now();
    };
    const vol = el.querySelector('#mVol');
    let volT = null;
    vol.addEventListener('input', () => {
        clearTimeout(volT);
        volT = setTimeout(() => window.isle.volSet(+vol.value), 90);
    });
    window.isle.volGet().then(v => { if (v && typeof v.volume === 'number') vol.value = v.volume; });
    if (music.els.pane) wirePane(music.els.pane);
    updateMusicUi();
}
function wirePane(pane) {
    pane.querySelectorAll('.ln[data-i]').forEach(ln => ln.onclick = () =>
        window.isle.musicCmd('seek', music.lines[+ln.dataset.i].t));
}
RENDER.music = (el) => {
    const info = music.info;
    if (!info || !info.hasSession) {
        el.innerHTML = '<p class="muted" style="text-align:center;margin-top:52px">No music playing.<br>Start something in Spotify, your browser or Media Player.</p>';
        music.els = null;
        return;
    }
    if (music.lyricsOn) {
        el.innerHTML = `
          <div class="split">
            <div class="mLeft">
              <div class="row">
                ${info.art ? `<img class="mArtS" src="data:${info.artType || 'image/jpeg'};base64,${info.art}">` : '<div class="mArtSPh">♪</div>'}
                <div style="min-width:0">
                  <div class="mTitle" style="font-size:14.5px">${esc(info.title) || 'Unknown title'}</div>
                  <div class="mArtist" style="font-size:12.5px">${esc(info.artist)}</div>
                </div>
              </div>
              ${musicControlsHtml(info)}
            </div>
            <div class="mLyrics" id="mPane">${paneLinesHtml()}</div>
          </div>`;
    } else {
        el.innerHTML = `
          <div class="row" style="align-items:flex-start">
            ${info.art ? `<img class="mArt" src="data:${info.artType || 'image/jpeg'};base64,${info.art}">` : '<div class="mArtPh">♪</div>'}
            <div style="min-width:0;padding-top:9px">
              <div class="mTitle">${esc(info.title) || 'Unknown title'}</div>
              <div class="mArtist">${esc(info.artist)}${info.album ? ` · ${esc(info.album)}` : ''}</div>
            </div>
          </div>
          ${musicControlsHtml(info)}`;
    }
    wireMusicControls(el);
};
function updateMusicUi() {
    if (!music.els || !music.info) return;
    const p = curPos(), d = music.info.duration || 0;
    music.els.fill.style.width = d ? `${(p / d) * 100}%` : '0%';
    music.els.pos.textContent = fmtClock(p);
    music.els.dur.textContent = d ? `-${fmtClock(Math.max(0, d - p))}` : '0:00';
    music.els.play.textContent = music.info.playing ? '⏸' : '▶';
    if (music.els.pane) {
        const idx = activeLineIdx();
        if (idx !== updateMusicUi._paneIdx || updateMusicUi._paneState !== music.lyricsState) {
            updateMusicUi._paneIdx = idx;
            updateMusicUi._paneState = music.lyricsState;
            music.els.pane.innerHTML = paneLinesHtml();
            wirePane(music.els.pane);
        }
    }
}

RENDER.pomodoro = (el) => {
    const modes = [['Focus', S.pomo.focus], ['Short Break', S.pomo.short], ['Long Break', S.pomo.long]];
    el.innerHTML = `
      <div class="pomoTop">
        <div class="ringWrap" id="pRing"><div class="ringIn">⏱</div></div>
        <div>
          <div class="pomoTime" id="pTime"></div>
          <div class="row" style="gap:7px"><span class="muted" id="pState"></span>
          <button id="pReset" title="Reset" style="color:#a9a9b3;font-size:14px">⟳</button></div>
        </div>
      </div>
      <div class="pomoModes">
        ${modes.map(([n, m]) => `<button class="modePill" data-n="${n}" data-m="${m}">${n === 'Short Break' ? '☕' : n === 'Long Break' ? '🌙' : '🎯'} ${n}</button>`).join('')}
        <button class="playPill" id="pToggle">▶</button>
      </div>`;
    const time = el.querySelector('#pTime'), state = el.querySelector('#pState'),
        tog = el.querySelector('#pToggle'), ring = el.querySelector('#pRing');
    const draw = () => {
        time.textContent = fmtClock(pomo.left);
        state.textContent = pomo.running ? `${pomo.label} — running` : pomo.label === 'Focus' ? 'Focus time' : pomo.label;
        tog.textContent = pomo.running ? '⏸' : '▶';
        const pct = pomo.dur ? ((1 - pomo.left / pomo.dur) * 100) : 0;
        ring.style.background = `conic-gradient(#ff453a ${pct}%, #2c2c33 0)`;
        el.querySelectorAll('.modePill').forEach(b => b.classList.toggle('active', b.dataset.n === pomo.label));
    };
    el.querySelectorAll('.modePill').forEach(b => b.onclick = () => {
        pomo.label = b.dataset.n;
        pomo.dur = +b.dataset.m * 60;
        pomo.left = pomo.dur;
        pomo.running = false;
        draw(); renderPill(true);
    });
    tog.onclick = () => {
        pomo.running = !pomo.running;
        if (pomo.running) pomo.endAt = Date.now() + pomo.left * 1000;
        draw(); renderPill(true);
    };
    el.querySelector('#pReset').onclick = () => { pomo.running = false; pomo.left = pomo.dur; draw(); renderPill(true); };
    pomo.draw = draw;
    draw();
};

RENDER.water = (el) => {
    if (data.waterlog.day !== todayKey()) {
        data.waterlog = {day: todayKey(), drinks: []};
        save('waterlog');
    }
    const w = data.waterlog;
    const total = w.drinks.reduce((a, d) => a + d.ml, 0);
    const pct = Math.min(100, Math.round(total / S.water.goal * 100));
    const edges = [6, 9, 12, 15, 18, 21];
    const buckets = edges.map((h, i) => {
        const end = i + 1 < edges.length ? edges[i + 1] : 24;
        return w.drinks.filter(d => d.h >= h && d.h < end).reduce((a, d) => a + d.ml, 0);
    });
    const max = Math.max(...buckets, 1);
    const litres = (ml) => (ml / 1000).toFixed(2).replace(/\.?0+$/, '');
    el.innerHTML = `
      <div class="waterWrap">
        <div style="flex:1">
          <div class="wBig">💧 ${litres(total)}<span> / ${litres(S.water.goal)} L</span></div>
          <p class="muted" style="margin:5px 0 13px">Today · <b style="color:#f2f2f2">${pct}%</b>${pct >= 100 ? ' · Goal reached 🎉' : ''}</p>
          <div class="row">
            <button class="btn" id="wDrink">Drink</button>
            <button class="btn ghost" id="wUndo">Undo</button>
            <span class="muted">${S.water.bottle} ml a drink</span>
          </div>
        </div>
        <div class="chart">${buckets.map((v, i) => `
          <div class="col"><div class="colBar" style="height:${Math.max(3, (v / max) * 100)}%"></div><div class="colLb">${String(edges[i]).padStart(2, '0')}</div></div>`).join('')}
        </div>
      </div>`;
    el.querySelector('#wDrink').onclick = () => {
        const n = new Date();
        w.drinks.push({h: n.getHours() + n.getMinutes() / 60, ml: S.water.bottle});
        save('waterlog');
        lastWaterReminder = '';
        showPage('water');
    };
    el.querySelector('#wUndo').onclick = () => { w.drinks.pop(); save('waterlog'); showPage('water'); };
};

// ---- shelf ----
const statCache = new Map();
let shelfSel = new Set();
let shelfAddText = false;
const fmtSize = (n) => n >= 1048576 ? `${(n / 1048576).toFixed(1)} MB` : n >= 1024 ? `${Math.round(n / 1024)} KB` : `${n} B`;
RENDER.shelf = (el) => {
    if (data.shelf.length) {
        setStripAction([
            ...(shelfSel.size ? [{label: `Remove ${shelfSel.size}`, fn: () => {
                data.shelf = data.shelf.filter(it => !shelfSel.has(it.id));
                shelfSel.clear(); save('shelf'); showPage('shelf');
            }}] : []),
            {label: 'Clear', fn: (e) => {
                if (e.target.dataset.armed) { data.shelf = []; shelfSel.clear(); save('shelf'); showPage('shelf'); }
                else { e.target.dataset.armed = '1'; e.target.textContent = 'Sure?'; }
            }},
        ]);
    }
    if (!data.shelf.length && !shelfAddText) {
        el.innerHTML = `<div class="shelfZone"><div style="font-size:19px">📥</div>
          <div style="font-size:13.5px;font-weight:650">Drop files here</div>
          <div class="muted" style="font-size:11px">Drag out anytime · click to open or copy</div></div>`;
        return;
    }
    el.innerHTML = `
      ${shelfAddText
        ? '<div class="row" style="margin-bottom:8px"><input type="text" id="sTextIn" placeholder="Type a note and press Enter — click it later to copy" style="flex:1"></div>'
        : '<div style="margin-bottom:6px"><button id="sAddTx" class="muted" style="font-size:12px">+ Text card</button></div>'}
      <div id="sList"></div>`;
    const addBtn = el.querySelector('#sAddTx');
    if (addBtn) addBtn.onclick = () => { shelfAddText = true; showPage('shelf'); };
    const inp = el.querySelector('#sTextIn');
    if (inp) {
        inp.focus();
        inp.addEventListener('keydown', (e) => {
            if (e.key !== 'Enter') return;
            const v = inp.value.trim();
            if (v) data.shelf.unshift({id: Date.now(), kind: 'text', text: v});
            shelfAddText = false;
            save('shelf');
            showPage('shelf');
        });
    }
    const list = el.querySelector('#sList');
    const draw = () => {
        list.innerHTML = '';
        for (const it of data.shelf) {
            const row = document.createElement('div');
            row.className = 'sRow' + (shelfSel.has(it.id) ? ' sel' : '');
            row.draggable = it.kind === 'file';
            if (it.kind === 'text') {
                row.innerHTML = `<span class="ic">📝</span><span class="txPrev"></span><span class="x" title="Remove">✕</span>`;
                row.querySelector('.txPrev').textContent = it.text;
                row.title = 'Click to copy';
            } else {
                const st = statCache.get(it.path);
                const name = it.path.split(/[\\/]/).pop();
                const icon = [[/\.(png|jpe?g|gif|webp)$/i, '🖼️'], [/\.(mp3|wav|flac)$/i, '🎵'],
                    [/\.(mp4|mkv|mov)$/i, '🎬'], [/\.(zip|rar|7z)$/i, '🗜️'], [/\.(pdf|docx?|txt)$/i, '📄']]
                    .find(([re]) => re.test(name))?.[1] || '📄';
                if (st && !st.exists) row.classList.add('missing');
                row.innerHTML = `<span class="ic">${st && st.isDir ? '📂' : icon}</span><span class="nm"></span>
                  <span class="sz">${st ? (st.exists ? (st.isDir ? 'Folder' : fmtSize(st.size)) : 'missing') : '…'}</span><span class="x" title="Remove">✕</span>`;
                row.querySelector('.nm').textContent = name;
                row.title = it.path;
                if (!st) window.isle.stat(it.path).then(s2 => { statCache.set(it.path, s2); draw(); });
            }
            row.addEventListener('click', (e) => {
                if (e.target.classList.contains('x')) {
                    data.shelf = data.shelf.filter(x => x.id !== it.id);
                    shelfSel.delete(it.id);
                    save('shelf'); showPage('shelf');
                    return;
                }
                if (e.ctrlKey || e.shiftKey) {
                    shelfSel.has(it.id) ? shelfSel.delete(it.id) : shelfSel.add(it.id);
                    showPage('shelf');
                    return;
                }
                if (it.kind === 'text') navigator.clipboard.writeText(it.text).then(() => showPopup('📋', 'Text copied'));
                else window.isle.openPath(it.path);
            });
            row.addEventListener('dragstart', () => {
                const chosen = shelfSel.has(it.id)
                    ? data.shelf.filter(x => shelfSel.has(x.id) && x.kind === 'file').map(x => x.path)
                    : [it.path];
                window.isle.dragStart(chosen);
            });
            list.appendChild(row);
        }
    };
    draw();
};

// drag files onto the notch to shelve them
const notchEl = $('#notch');
notchEl.addEventListener('dragover', (e) => { e.preventDefault(); $('#card').classList.add('dropping'); });
notchEl.addEventListener('dragleave', () => $('#card').classList.remove('dropping'));
notchEl.addEventListener('drop', (e) => {
    e.preventDefault();
    $('#card').classList.remove('dropping');
    let added = false;
    for (const f of e.dataTransfer.files) {
        const p = window.isle.pathForFile(f);
        if (p && !data.shelf.some(it => it.path === p)) {
            data.shelf.unshift({id: Date.now() + Math.random(), kind: 'file', path: p});
            added = true;
        }
    }
    if (!e.dataTransfer.files.length) {
        const text = e.dataTransfer.getData('text/plain');
        if (text) { data.shelf.unshift({id: Date.now(), kind: 'text', text}); added = true; }
    }
    if (added) {
        save('shelf');
        showPopup('🗂️', 'Added to the Shelf');
        if (expanded) showPage('shelf'); else fit();
    }
});

// ---- todo ----
let todoAdd = false, todoEdit = false;
RENDER.todo = (el) => {
    const done = data.todos.filter(t => t.done).length;
    let left;
    if (todoAdd) {
        left = `
          <input type="text" id="tIn" placeholder="Task name" style="width:100%">
          <div class="row" style="margin-top:6px;gap:6px"><input type="time" id="tTime" style="flex:1">
          <span class="row" style="gap:4px">${TODO_COLORS.map((c, i) =>
            `<button class="colorDot${i === 0 ? ' active' : ''}" data-c="${c}" style="background:${c}"></button>`).join('')}</span></div>
          <button class="tBtn" id="tSave">✓ Add Task</button>
          <button class="tBtn" id="tCancel">Cancel</button>`;
    } else {
        left = `
          <div class="tBig">${done}/${data.todos.length}</div>
          <div class="muted" style="font-size:11px">Task Done Today</div>
          <button class="tBtn" id="tAddBtn">⊕ Add Task</button>
          <button class="tBtn${todoEdit ? ' active' : ''}" id="tEditBtn">✎ Edit Task</button>`;
    }
    el.innerHTML = `
      ${dueTask ? `<div class="dueBanner"><b>⏰ Time's up!</b> — ${esc(dueTask.text)}
        <span class="row" style="margin-top:7px">
          <button class="btn" id="dueDone" style="padding:5px 14px">Done</button>
          <button class="btn ghost" id="dueSnooze" style="padding:5px 14px">Snooze 10 min</button>
        </span></div>` : ''}
      <div class="todoWrap">
        <div class="todoLeft">${left}</div>
        <div class="todoRight" id="tList"></div>
      </div>`;
    if (todoAdd) {
        let color = TODO_COLORS[0];
        el.querySelectorAll('.colorDot').forEach(d => d.onclick = () => {
            color = d.dataset.c;
            el.querySelectorAll('.colorDot').forEach(x => x.classList.toggle('active', x === d));
        });
        el.querySelector('#tCancel').onclick = () => { todoAdd = false; showPage('todo'); };
        el.querySelector('#tSave').onclick = () => {
            const v = el.querySelector('#tIn').value.trim();
            if (!v) return;
            data.todos.unshift({id: Date.now(), text: v, time: el.querySelector('#tTime').value || '',
                color, done: false, firedDay: '', snoozeUntil: 0});
            todoAdd = false;
            save('todos');
            showPage('todo');
        };
        el.querySelector('#tIn').focus();
    } else {
        el.querySelector('#tAddBtn').onclick = () => { todoAdd = true; showPage('todo'); };
        el.querySelector('#tEditBtn').onclick = () => { todoEdit = !todoEdit; showPage('todo'); };
    }
    if (dueTask) {
        el.querySelector('#dueDone').onclick = () => { dueTask.done = true; dueTask = null; save('todos'); showPage('todo'); };
        el.querySelector('#dueSnooze').onclick = () => {
            dueTask.snoozeUntil = Date.now() + 600000; dueTask.firedDay = '';
            dueTask = null; save('todos'); showPage('todo');
        };
    }
    const list = el.querySelector('#tList');
    const items = [...data.todos].sort((a, b) => (a.done - b.done) || String(a.time).localeCompare(String(b.time)));
    if (!items.length) list.innerHTML = '<p class="muted" style="padding:8px 4px">No tasks yet.</p>';
    for (const t of items) {
        const row = document.createElement('div');
        row.className = 'tdRow' + (t.done ? ' done' : '');
        row.innerHTML = `<button class="ck">${t.done ? '✓' : ''}</button><span class="tx"></span>
          ${t.time ? `<span class="tm">${esc(t.time)}</span>` : ''}
          ${todoEdit ? '<button class="x" title="Delete">✕</button>' : ''}`;
        const tx = row.querySelector('.tx');
        tx.textContent = t.text;
        tx.style.borderLeft = `3px solid ${t.color || TODO_COLORS[0]}`;
        tx.style.paddingLeft = '7px';
        row.querySelector('.ck').onclick = () => { t.done = !t.done; save('todos'); showPage('todo'); };
        const x = row.querySelector('.x');
        if (x) x.onclick = () => {
            data.todos = data.todos.filter(z => z.id !== t.id);
            if (dueTask && dueTask.id === t.id) dueTask = null;
            save('todos'); showPage('todo');
        };
        if (todoEdit) {
            tx.title = 'Click to edit';
            tx.onclick = () => {
                const inp = document.createElement('input');
                inp.type = 'text'; inp.value = t.text; inp.style.flex = '1';
                tx.replaceWith(inp);
                inp.focus();
                inp.onblur = () => { t.text = inp.value.trim() || t.text; save('todos'); showPage('todo'); };
                inp.onkeydown = (e) => { if (e.key === 'Enter') inp.blur(); };
            };
        }
        list.appendChild(row);
    }
    fit();
};

// ---- calendar ----
let calY, calM;
RENDER.calendar = (el) => {
    const now = new Date();
    if (calY === undefined) { calY = now.getFullYear(); calM = now.getMonth(); }
    const first = new Date(calY, calM, 1);
    const startDay = (first.getDay() + 6) % 7;
    const days = new Date(calY, calM + 1, 0).getDate();
    const prevDays = new Date(calY, calM, 0).getDate();
    let cells = ['Mo', 'Tu', 'We', 'Th', 'Fr', 'Sa', 'Su'].map(d => `<div class="dow">${d}</div>`).join('');
    for (let i = 0; i < startDay; i++) cells += `<div class="day dim">${prevDays - startDay + 1 + i}</div>`;
    for (let d = 1; d <= days; d++) {
        const today = d === now.getDate() && calM === now.getMonth() && calY === now.getFullYear();
        cells += `<div class="day${today ? ' today' : ''}">${d}</div>`;
    }
    el.innerHTML = `<div class="calHead"><button id="calPrev">‹</button>
      <b>${first.toLocaleString(undefined, {month: 'long'})} ${calY}</b>
      <button id="calNext">›</button></div><div class="calGrid">${cells}</div>`;
    el.querySelector('#calPrev').onclick = () => { calM--; if (calM < 0) { calM = 11; calY--; } showPage('calendar'); };
    el.querySelector('#calNext').onclick = () => { calM++; if (calM > 11) { calM = 0; calY++; } showPage('calendar'); };
};

// ---------------- reminders ----------------
function checkReminders() {
    const now = new Date();
    if (S.todoRemind !== false) {
        for (const t of data.todos) {
            if (t.done || !t.time || t.firedDay === todayKey() || Date.now() < (t.snoozeUntil || 0)) continue;
            const [h, m] = t.time.split(':').map(Number);
            const due = new Date();
            due.setHours(h, m, 0, 0);
            if (now >= due) {
                t.firedDay = todayKey();
                dueTask = t;
                save('todos');
                showPopup('⏰', `Time's up! — ${t.text}`, 8000);
                window.isle.notify("Isle — Time's up!", t.text);
                openCard('todo');
            }
        }
    }
    if (S && S.water.remind && data.waterlog.day === todayKey()) {
        const hourKey = `${todayKey()} ${now.getHours()}`;
        const total = data.waterlog.drinks.reduce((a, d) => a + d.ml, 0);
        const last = data.waterlog.drinks.length
            ? Math.max(...data.waterlog.drinks.map(d => d.h)) : 0;
        const nowH = now.getHours() + now.getMinutes() / 60;
        if (now.getHours() >= 8 && now.getHours() <= 21 && total < S.water.goal &&
            nowH - last >= 1 && lastWaterReminder !== hourKey) {
            lastWaterReminder = hourKey;
            showPopup('💧', 'Time to drink some water', 5000);
        }
    }
}

// ---------------- settings application ----------------
function applySettings() {
    document.body.style.zoom = S.scale || 1;
    lastSentSize = '';
    if (!visiblePages().some(p => p.id === current)) current = visiblePages()[0]?.id || 'music';
    pomo.dur = (S.pomo.focus || 25) * 60;
    pillSig = '';
    renderPill(true);
    if (expanded) showPage(current); else fit();
}

// ---------------- tickers ----------------
setInterval(() => {
    if (pomo.running) {
        pomo.left = (pomo.endAt - Date.now()) / 1000;
        if (pomo.left <= 0) {
            pomo.running = false;
            pomo.left = pomo.dur;
            chime();
            window.isle.notify('Isle — Pomodoro', `${pomo.label} finished.`);
            showPopup('⏰', `${pomo.label} finished`, 6000);
        }
        if (expanded && current === 'pomodoro' && pomo.draw) pomo.draw();
    }
    tickStrip();
    updateBell();
    renderPill();
    checkReminders();
}, 1000);
setInterval(() => {
    if (expanded && current === 'music') updateMusicUi();
    if (expanded && current === 'pomodoro' && pomo.running && pomo.draw) pomo.draw();
}, 250);
setInterval(refreshMusic, 1500);

// ---------------- boot ----------------
(async () => {
    S = await window.isle.settingsGet();
    window.isle.onSettings((s) => { S = s; applySettings(); });
    const saved = await window.isle.storeGet();
    data = {...data, ...saved};
    if (data.waterlog.day !== todayKey()) data.waterlog = {day: todayKey(), drinks: []};
    pomo.dur = (S.pomo.focus || 25) * 60;
    pomo.left = pomo.dur;
    if (navigator.getBattery) {
        try {
            battery = await navigator.getBattery();
            battery.addEventListener('chargingchange', () => {
                const pct = Math.round(battery.level * 100);
                showPopup(battery.charging ? '⚡' : '🔋',
                    battery.charging ? `Charging · ${pct}%` : `On battery · ${pct}%`);
                renderPill(true);
            });
        } catch { battery = null; }
    }
    window.addEventListener('online', () => showPopup('📶', 'Back online'));
    window.addEventListener('offline', () => showPopup('⚠️', 'Wi-Fi disconnected'));
    document.body.style.zoom = S.scale || 1;
    current = visiblePages()[0]?.id || 'music';
    tickStrip();
    renderPill(true);
    refreshMusic();
})();
