// Isle for Windows — renderer, modelled on the Nusa app.
// Collapsed: clock (or the mascot's eyes when idle), the playing song,
// a running pomodoro, or a short popup. Hover opens the card; pages are
// Music, Pomodoro, Water, Shelf, Todo, Calendar. Music is read from the
// real Windows media sessions by the main process — never faked.
const $ = (s) => document.querySelector(s);

const PAGES = [
    {id: 'music', icon: '🎵', name: 'Music'},
    {id: 'pomodoro', icon: '🎯', name: 'Pomodoro'},
    {id: 'water', icon: '💧', name: 'Water'},
    {id: 'shelf', icon: '🗂️', name: 'Shelf'},
    {id: 'todo', icon: '✔️', name: 'Todo'},
    {id: 'calendar', icon: '📅', name: 'Calendar'},
];
const TODO_COLORS = ['#ffb454', '#ff5a52', '#7ed492', '#57a7ff', '#c792ff'];

let S = null; // settings (owned by main, pushed on change)
let data = {todos: [], shelf: [], waterlog: {day: '', drinks: []}};
let expanded = false;
let current = 'music';
let mode = ''; // last window mode sent to main
let popup = null; // {icon, text, until}
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

// ---------------- window modes ----------------
function lyricsVisible() {
    return S && S.lyricsPill && music.lyricsOn && music.lines.length > 0 &&
        music.info && music.info.hasSession;
}
function syncMode() {
    let m;
    if (expanded) m = lyricsVisible() ? 'cardLyrics' : 'card';
    else if (pillKind === 'line') m = 'line';
    else m = lyricsVisible() ? 'pillLyrics' : 'pill';
    $('#lyricsPill').hidden = !lyricsVisible();
    if (m !== mode) {
        mode = m;
        window.isle.setMode(m);
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
    lines: [], lyricsKey: null, lyricsOn: false, activeLine: -1, els: null,
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
            music.info = null; music.key = null; music.lines = []; music.lyricsKey = null;
            renderPill(true); syncMode();
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
        music.lines = []; music.lyricsKey = null; music.activeLine = -1;
        fetchLyrics(info);
        renderPill(true); syncMode();
        if (expanded && current === 'music') showPage('music');
    }
}
async function fetchLyrics(info) {
    const key = music.key;
    try {
        const q = new URLSearchParams({
            track_name: info.title || '', artist_name: info.artist || '',
            duration: String(Math.round(info.duration || 0)),
        });
        const r = await fetch(`https://lrclib.net/api/get?${q}`);
        if (!r.ok) return;
        const j = await r.json();
        if (music.key !== key || !j.syncedLyrics) return;
        music.lines = j.syncedLyrics.split('\n').map(line => {
            const m = line.match(/\[(\d+):(\d+(?:\.\d+)?)\](.*)/);
            return m ? {t: +m[1] * 60 + +m[2], text: m[3].trim()} : null;
        }).filter(l => l && l.text);
        music.lyricsKey = key;
        renderLyricsPill(true);
        if (expanded && current === 'music') showPage('music');
        syncMode();
    } catch { /* offline or no match: the page just shows no lyrics */ }
}
function activeLineIdx() {
    const p = curPos();
    let idx = -1;
    for (let i = 0; i < music.lines.length; i++) {
        if (music.lines[i].t <= p) idx = i; else break;
    }
    return idx;
}
function renderLyricsPill(force) {
    const el = $('#lyricsPill');
    if (!lyricsVisible()) { el.hidden = true; return; }
    el.hidden = false;
    const idx = activeLineIdx();
    if (idx === music.activeLine && !force) return;
    music.activeLine = idx;
    const at = (i) => (music.lines[i] ? esc(music.lines[i].text) : '&nbsp;');
    el.innerHTML = [idx - 1, idx, idx + 1].map((i, k) =>
        `<div class="ln${k === 1 ? ' cur' : ''}" data-i="${i}">${i >= 0 ? at(i) : '&nbsp;'}</div>`).join('');
    el.querySelectorAll('.ln').forEach(ln => ln.onclick = () => {
        const i = +ln.dataset.i;
        if (music.lines[i]) window.isle.musicCmd('seek', music.lines[i].t);
    });
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
        html = `<span style="font-size:16px">${popup.icon}</span><span class="popTx">${esc(popup.text)}</span>`;
    } else if (pomo.running) {
        kind = 'pomo';
        sig = `p${Math.ceil(pomo.left)}`;
        html = `<span style="font-size:15px">🎯</span><span class="clock">${fmtClock(pomo.left)}</span><span class="date">${esc(pomo.label)}</span>`;
    } else if (music.info && music.info.hasSession) {
        kind = 'music';
        const p = curPos(), d = music.info.duration || 0;
        sig = `m${music.key}|${Math.floor(p)}`;
        html = `${music.info.art
            ? `<img class="miniArt" src="data:${music.info.artType || 'image/jpeg'};base64,${music.info.art}">`
            : '<span class="miniArtPh">♪</span>'}
          <span class="miniTx"><div class="miniTitle">${esc(music.info.title) || 'Unknown title'}</div>
          <div class="miniSub">${esc(music.info.artist)}${d ? ` · ${fmtClock(p)} · -${fmtClock(Math.max(0, d - p))}` : ''}</div></span>
          <span style="font-size:13px">${music.info.playing ? '▶' : '⏸'}</span>`;
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
        sig = `c${d.getMinutes()}`;
        html = `<span class="clock">${d.toLocaleTimeString([], {hour: '2-digit', minute: '2-digit'})}</span>
                <span class="date">${d.toLocaleDateString([], {weekday: 'short', day: 'numeric', month: 'short'})}</span>`;
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
    syncMode();
}

// ---------------- expand / collapse ----------------
let openT = null, closeT = null;
function openCard(page) {
    if (page) current = page;
    if (!visiblePages().some(p => p.id === current)) current = visiblePages()[0]?.id || 'music';
    expanded = true;
    $('#pill').style.display = 'none';
    $('#line').hidden = true;
    $('#card').hidden = false;
    showPage(current);
    syncMode();
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
$('#gearBtn').addEventListener('click', () => window.isle.openSettings());
$('#card').addEventListener('wheel', (e) => {
    const now = Date.now();
    if (now - (openCard._wheelAt || 0) < 350) return;
    openCard._wheelAt = now;
    const pages = visiblePages();
    let i = pages.findIndex(p => p.id === current);
    i = (i + (e.deltaY > 0 ? 1 : -1) + pages.length) % pages.length;
    showPage(pages[i].id);
}, {passive: true});
document.addEventListener('keydown', (e) => {
    if (!expanded || e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') return;
    const pages = visiblePages();
    let i = pages.findIndex(p => p.id === current);
    if (e.key === 'ArrowRight') showPage(pages[(i + 1) % pages.length].id);
    if (e.key === 'ArrowLeft') showPage(pages[(i - 1 + pages.length) % pages.length].id);
});

// ---------------- navigation ----------------
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
function setHead(title, actions) {
    $('#pageTitle').textContent = title;
    const slot = $('#headAction');
    slot.innerHTML = '';
    for (const a of actions || []) {
        const b = document.createElement('button');
        b.textContent = a.label;
        if (a.warn) b.className = 'warn';
        b.onclick = a.fn;
        slot.appendChild(b);
    }
}
function showPage(id) {
    current = id;
    const body = $('#pageBody');
    body.innerHTML = '';
    const el = document.createElement('div');
    el.className = 'page';
    body.appendChild(el);
    RENDER[id](el);
    buildNav();
}

// ---------------- pages ----------------
const RENDER = {};

RENDER.music = (el) => {
    const info = music.info;
    setHead('Music', []);
    if (!info || !info.hasSession) {
        el.innerHTML = '<p class="muted" style="text-align:center;margin-top:60px">No music playing.<br>Start something in Spotify, your browser or Media Player.</p>';
        music.els = null;
        return;
    }
    el.innerHTML = `
      <div class="row" style="align-items:flex-start">
        ${info.art ? `<img class="mArt" src="data:${info.artType || 'image/jpeg'};base64,${info.art}">` : '<div class="mArtPh">♪</div>'}
        <div style="min-width:0;padding-top:6px">
          <div class="mTitle">${esc(info.title) || 'Unknown title'}</div>
          <div class="mArtist">${esc(info.artist)}${info.album ? ` · ${esc(info.album)}` : ''}</div>
        </div>
      </div>
      <div style="margin-top:20px">
        <div class="seek" id="mSeek"><div id="mSeekFill"></div></div>
        <div class="mTimes"><span id="mPos">0:00</span><span id="mDur">0:00</span></div>
      </div>
      <div class="mCtl">
        <button id="mPrev" title="Previous">⏮</button>
        <button id="mPlay" class="play" title="Play / pause">${info.playing ? '⏸' : '▶'}</button>
        <button id="mNext" title="Next">⏭</button>
        <button id="mLyr" class="lyrBtn${music.lyricsOn ? ' active' : ''}" title="Synced lyrics (LRCLIB)">🎤 Lyrics</button>
        <span class="volWrap"><span style="font-size:15px">🔊</span><input type="range" id="mVol" min="0" max="100"></span>
      </div>
      <div class="lyrList" id="mLyrList"></div>`;
    music.els = {
        seek: el.querySelector('#mSeek'), fill: el.querySelector('#mSeekFill'),
        pos: el.querySelector('#mPos'), dur: el.querySelector('#mDur'),
        play: el.querySelector('#mPlay'), list: el.querySelector('#mLyrList'),
    };
    el.querySelector('#mPrev').onclick = () => window.isle.musicCmd('prev').then(() => setTimeout(refreshMusic, 450));
    el.querySelector('#mNext').onclick = () => window.isle.musicCmd('next').then(() => setTimeout(refreshMusic, 450));
    music.els.play.onclick = () => window.isle.musicCmd('playpause').then(() => setTimeout(refreshMusic, 350));
    el.querySelector('#mLyr').onclick = (e) => {
        music.lyricsOn = !music.lyricsOn;
        e.currentTarget.classList.toggle('active', music.lyricsOn);
        drawLyricsList();
        renderLyricsPill(true);
        syncMode();
    };
    music.els.seek.onclick = (e) => {
        const d = music.info.duration || 0;
        if (!d) return;
        const r = music.els.seek.getBoundingClientRect();
        window.isle.musicCmd('seek', Math.round(((e.clientX - r.left) / r.width) * d));
        music.pos = ((e.clientX - r.left) / r.width) * d;
        music.stamp = Date.now();
    };
    const vol = el.querySelector('#mVol');
    let volT = null;
    vol.addEventListener('input', () => {
        clearTimeout(volT);
        volT = setTimeout(() => window.isle.volSet(+vol.value), 90);
    });
    window.isle.volGet().then(v => { if (v && typeof v.volume === 'number') vol.value = v.volume; });
    drawLyricsList();
    updateMusicUi();
};
function drawLyricsList() {
    const list = music.els && music.els.list;
    if (!list) return;
    if (!music.lyricsOn) { list.innerHTML = ''; return; }
    if (!music.lines.length) {
        list.innerHTML = '<p class="muted">No synced lyrics found for this song.</p>';
        return;
    }
    list.innerHTML = music.lines.map((l, i) => `<div class="ln" data-i="${i}">${esc(l.text)}</div>`).join('');
    list.querySelectorAll('.ln').forEach(ln => ln.onclick = () =>
        window.isle.musicCmd('seek', music.lines[+ln.dataset.i].t));
}
function updateMusicUi() {
    if (!music.els || !music.info) return;
    const p = curPos(), d = music.info.duration || 0;
    music.els.fill.style.width = d ? `${(p / d) * 100}%` : '0%';
    music.els.pos.textContent = fmtClock(p);
    music.els.dur.textContent = fmtClock(d);
    music.els.play.textContent = music.info.playing ? '⏸' : '▶';
    if (music.lyricsOn && music.lines.length) {
        const idx = activeLineIdx();
        const nodes = music.els.list.querySelectorAll('.ln');
        nodes.forEach((n, i) => n.classList.toggle('cur', i === idx));
        if (idx !== updateMusicUi._last && nodes[idx]) {
            nodes[idx].scrollIntoView({block: 'center', behavior: 'smooth'});
            updateMusicUi._last = idx;
        }
    }
}

RENDER.pomodoro = (el) => {
    setHead('Pomodoro', []);
    const modes = [['Focus', S.pomo.focus], ['Short Break', S.pomo.short], ['Long Break', S.pomo.long]];
    el.innerHTML = `
      <div class="row" style="justify-content:center;gap:8px">
        ${modes.map(([n, m]) => `<button class="chipBtn" data-n="${n}" data-m="${m}">${n} · ${m}m</button>`).join('')}
      </div>
      <div class="focusTime" id="pTime"></div>
      <p class="muted" style="text-align:center" id="pState"></p>
      <div class="row" style="justify-content:center;margin-top:18px;gap:10px">
        <button class="btn" id="pToggle">Start</button>
        <button class="btn ghost" id="pReset">Reset</button>
      </div>`;
    const time = el.querySelector('#pTime'), state = el.querySelector('#pState'), tog = el.querySelector('#pToggle');
    const draw = () => {
        time.textContent = fmtClock(pomo.left);
        state.textContent = pomo.running ? `${pomo.label} — running` : pomo.label;
        tog.textContent = pomo.running ? 'Pause' : 'Start';
        el.querySelectorAll('.chipBtn').forEach(b =>
            b.classList.toggle('active', b.dataset.n === pomo.label));
    };
    el.querySelectorAll('.chipBtn').forEach(b => b.onclick = () => {
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
    pomo.dur = (S.pomo.focus || 25) * 60;
    if (!pomo.running && pomo.left === 25 * 60) pomo.left = pomo.dur;
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
    setHead('Water Reminder', []);
    el.innerHTML = `
      <div style="text-align:center;margin-top:8px">
        <div class="big">💧 ${(total / 1000).toFixed(2).replace(/\.?0+$/, '') || '0'}<span style="font-size:19px;color:#a9a9b3"> / ${(S.water.goal / 1000)} L</span></div>
        <p class="muted">Today · <b style="color:#f2f2f2">${pct}%</b>${pct >= 100 ? ' · Goal reached 🎉' : ''}</p>
        <div class="chart">${buckets.map((v, i) => `
          <div class="col"><div class="colBar" style="height:${Math.max(3, (v / max) * 100)}%"></div><div class="colLb">${String(edges[i]).padStart(2, '0')}</div></div>`).join('')}
        </div>
        <div class="row" style="justify-content:center;gap:10px;margin-top:12px">
          <button class="btn" id="wDrink">Drink ${S.water.bottle} ml</button>
          <button class="btn ghost" id="wUndo">Undo</button>
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
RENDER.shelf = (el) => {
    setHead('Shelf', [
        {label: '+ Text', fn: () => { shelfAddText = !shelfAddText; showPage('shelf'); }},
        ...(shelfSel.size ? [{label: `Remove ${shelfSel.size}`, warn: true, fn: () => {
            data.shelf = data.shelf.filter(it => !shelfSel.has(it.id));
            shelfSel.clear(); save('shelf'); showPage('shelf');
        }}] : []),
        {label: 'Clear', fn: (e) => {
            if (e.target.dataset.armed) { data.shelf = []; shelfSel.clear(); save('shelf'); showPage('shelf'); }
            else { e.target.dataset.armed = '1'; e.target.textContent = 'Sure?'; setTimeout(() => showPage('shelf'), 2600); }
        }},
    ]);
    el.innerHTML = `
      ${shelfAddText ? '<div class="row" style="margin-bottom:12px"><input type="text" id="sTextIn" placeholder="Type a note and press Enter — click it later to copy" style="flex:1"></div>' : ''}
      <div class="grid" id="sGrid"></div>
      ${data.shelf.length ? '' : '<p class="muted" style="text-align:center;margin-top:36px">Drop files here — drag them onto the notch.<br>Drag out anytime · click to open</p>'}`;
    const grid = el.querySelector('#sGrid');
    const draw = () => {
        grid.innerHTML = '';
        for (const it of data.shelf) {
            const t = document.createElement('div');
            t.className = 'tile' + (shelfSel.has(it.id) ? ' sel' : '');
            t.draggable = it.kind === 'file';
            if (it.kind === 'text') {
                t.innerHTML = `<div class="txPrev"></div><div class="nm">Text card</div>`;
                t.querySelector('.txPrev').textContent = it.text;
                t.title = 'Click to copy';
            } else {
                const st = statCache.get(it.path);
                const name = it.path.split(/[\\/]/).pop();
                const icon = [[/\.(png|jpe?g|gif|webp)$/i, '🖼️'], [/\.(mp3|wav|flac)$/i, '🎵'],
                    [/\.(mp4|mkv|mov)$/i, '🎬'], [/\.(zip|rar|7z)$/i, '🗜️'], [/\.(pdf|docx?|txt)$/i, '📄']]
                    .find(([re]) => re.test(name))?.[1] || '📁';
                if (st && !st.exists) t.classList.add('missing');
                t.innerHTML = `<div class="ic">${st && st.isDir ? '📂' : icon}</div><div class="nm"></div>
                  <div class="sz">${st ? (st.exists ? (st.isDir ? 'Folder' : fmtSize(st.size)) : 'missing') : '…'}</div>`;
                t.querySelector('.nm').textContent = name;
                t.title = it.path;
                if (!st) window.isle.stat(it.path).then(s2 => { statCache.set(it.path, s2); draw(); });
            }
            t.addEventListener('click', (e) => {
                if (e.ctrlKey || e.shiftKey) {
                    shelfSel.has(it.id) ? shelfSel.delete(it.id) : shelfSel.add(it.id);
                    showPage('shelf');
                    return;
                }
                if (it.kind === 'text') {
                    navigator.clipboard.writeText(it.text).then(() => showPopup('📋', 'Text copied'));
                } else {
                    window.isle.openPath(it.path);
                }
            });
            t.addEventListener('dragstart', () => {
                const chosen = shelfSel.has(it.id)
                    ? data.shelf.filter(x => shelfSel.has(x.id) && x.kind === 'file').map(x => x.path)
                    : [it.path];
                window.isle.dragStart(chosen);
            });
            grid.appendChild(t);
        }
    };
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
    draw();
};
let shelfAddText = false;
const fmtSize = (n) => n >= 1048576 ? `${(n / 1048576).toFixed(1)} MB` : n >= 1024 ? `${Math.round(n / 1024)} KB` : `${n} B`;

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
        if (expanded) showPage('shelf');
    }
});

// ---- todo ----
RENDER.todo = (el) => {
    setHead(`TodoList · ${data.todos.filter(t => t.done).length}/${data.todos.length} done`, []);
    let html = '';
    if (dueTask) {
        html += `<div class="dueBanner"><b>⏰ Time's up!</b> — ${esc(dueTask.text)}
          <div class="row" style="margin-top:9px">
            <button class="btn" id="dueDone">Done</button>
            <button class="btn ghost" id="dueSnooze">Snooze 10 min</button>
          </div></div>`;
    }
    html += `
      <div class="row" style="margin-bottom:6px">
        <input type="text" id="tIn" placeholder="Add Task" style="flex:1">
        <input type="time" id="tTime">
        <span class="row" style="gap:5px">${TODO_COLORS.map((c, i) =>
          `<button class="colorDot${i === 0 ? ' active' : ''}" data-c="${c}" style="background:${c}"></button>`).join('')}</span>
        <button class="btn" id="tAdd">Add</button>
      </div>
      <div id="tList"></div>`;
    el.innerHTML = html;
    let color = TODO_COLORS[0];
    el.querySelectorAll('.colorDot').forEach(d => d.onclick = () => {
        color = d.dataset.c;
        el.querySelectorAll('.colorDot').forEach(x => x.classList.toggle('active', x === d));
    });
    const add = () => {
        const v = el.querySelector('#tIn').value.trim();
        if (!v) return;
        data.todos.unshift({id: Date.now(), text: v, time: el.querySelector('#tTime').value || '',
            color, done: false, firedDay: '', snoozeUntil: 0});
        save('todos');
        showPage('todo');
    };
    el.querySelector('#tAdd').onclick = add;
    el.querySelector('#tIn').addEventListener('keydown', (e) => { if (e.key === 'Enter') add(); });
    if (dueTask) {
        el.querySelector('#dueDone').onclick = () => {
            dueTask.done = true; dueTask = null; save('todos'); showPage('todo');
        };
        el.querySelector('#dueSnooze').onclick = () => {
            dueTask.snoozeUntil = Date.now() + 600000; dueTask.firedDay = '';
            dueTask = null; save('todos'); showPage('todo');
        };
    }
    const list = el.querySelector('#tList');
    const items = [...data.todos].sort((a, b) => (a.done - b.done) || String(a.time).localeCompare(String(b.time)));
    if (!items.length) list.innerHTML = '<p class="muted" style="padding:10px 6px">No tasks yet.</p>';
    for (const t of items) {
        const row = document.createElement('div');
        row.className = 'todoItem' + (t.done ? ' done' : '');
        row.style.borderLeftColor = t.color || TODO_COLORS[0];
        row.innerHTML = `<input type="checkbox" ${t.done ? 'checked' : ''}><span class="tx"></span>
          ${t.time ? `<span class="tm">🕐 ${esc(t.time)}</span>` : ''}<button title="Delete">✕</button>`;
        const tx = row.querySelector('.tx');
        tx.textContent = t.text;
        tx.title = 'Double-click to edit';
        tx.ondblclick = () => {
            const inp = document.createElement('input');
            inp.type = 'text'; inp.value = t.text; inp.style.flex = '1';
            tx.replaceWith(inp);
            inp.focus();
            inp.onblur = () => { t.text = inp.value.trim() || t.text; save('todos'); showPage('todo'); };
            inp.onkeydown = (e) => { if (e.key === 'Enter') inp.blur(); };
        };
        row.querySelector('input').onchange = (e) => { t.done = e.target.checked; save('todos'); showPage('todo'); };
        row.querySelector('button').onclick = () => {
            data.todos = data.todos.filter(x => x.id !== t.id);
            if (dueTask && dueTask.id === t.id) dueTask = null;
            save('todos'); showPage('todo');
        };
        list.appendChild(row);
    }
};

// ---- calendar ----
let calY, calM;
RENDER.calendar = (el) => {
    setHead('Calendar', []);
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
    // todo due times
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
    // water reminder: hourly, daytime, only while behind goal
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
    if (!visiblePages().some(p => p.id === current)) current = visiblePages()[0]?.id || 'music';
    pomo.dur = (S.pomo.focus || 25) * 60;
    pillSig = '';
    renderPill(true);
    if (expanded) showPage(current);
}

// ---------------- tickers ----------------
setInterval(() => {
    // pomodoro countdown
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
    renderPill();
    renderLyricsPill();
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
            });
        } catch { battery = null; }
    }
    window.addEventListener('online', () => showPopup('📶', 'Back online'));
    window.addEventListener('offline', () => showPopup('⚠️', 'Wi-Fi disconnected'));
    document.body.style.zoom = S.scale || 1;
    current = visiblePages()[0]?.id || 'music';
    renderPill(true);
    refreshMusic();
})();
