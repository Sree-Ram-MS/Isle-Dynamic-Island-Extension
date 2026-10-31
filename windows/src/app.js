// Isle for Windows — renderer. Same pages and behaviour as the GNOME
// notch: hover to open, dots to switch, hover the dots for the icon bar.
const $ = (s) => document.querySelector(s);

const PAGES = [
    {id: 'power', icon: '🔋', name: 'Power'},
    {id: 'hardware', icon: '🖥️', name: 'Hardware'},
    {id: 'calendar', icon: '📅', name: 'Calendar'},
    {id: 'todo', icon: '✔️', name: 'Todo'},
    {id: 'focus', icon: '🎯', name: 'Focus'},
    {id: 'water', icon: '💧', name: 'Water'},
    {id: 'shelf', icon: '🗂️', name: 'Shelf'},
    {id: 'wallpaper', icon: '🖼️', name: 'Wallpaper'},
];

let store = {
    todos: [], water: {day: '', ml: 0, goal: 2000}, shelf: [],
    pages: {}, position: 'top', scale: 1, wpCurrent: '',
};
let current = 'power';
let inSettings = false;
let lastSys = null;
let battery = null;

const save = (key) => window.isle.storeSet(key, store[key]);
const visiblePages = () => PAGES.filter(p => store.pages[p.id] !== false);
const fmtBytes = (n) => `${(n / 1073741824).toFixed(1)} GB`;
const todayStr = () => new Date().toDateString();

// ---------------- expand / collapse ----------------
let expanded = false, openT = null, closeT = null;
function openCard() {
    if (expanded) return;
    expanded = true;
    window.isle.setExpanded(true);
    $('#pill').hidden = true;
    $('#card').hidden = false;
}
function closeCard() {
    if (!expanded) return;
    expanded = false;
    window.isle.setExpanded(false);
    $('#card').hidden = true;
    $('#pill').hidden = false;
}
$('#pill').addEventListener('mouseenter', () => {
    clearTimeout(closeT);
    openT = setTimeout(openCard, 120);
});
document.documentElement.addEventListener('mouseleave', () => {
    clearTimeout(openT);
    closeT = setTimeout(closeCard, 650);
});
document.documentElement.addEventListener('mouseenter', () => clearTimeout(closeT));
$('#closeBtn').addEventListener('click', closeCard);
$('#gearBtn').addEventListener('click', () => showSettings());

// ---------------- navigation: dots + hover icon bar ----------------
function buildNav() {
    const dots = $('#dots'), bar = $('#iconbar');
    dots.innerHTML = '';
    bar.innerHTML = '';
    for (const p of visiblePages()) {
        const d = document.createElement('button');
        d.className = 'dot' + (!inSettings && p.id === current ? ' active' : '');
        d.title = p.name;
        d.addEventListener('click', () => showPage(p.id));
        dots.appendChild(d);
        const b = document.createElement('button');
        b.textContent = p.icon;
        b.title = p.name;
        b.className = !inSettings && p.id === current ? 'active' : '';
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
    inSettings = false;
    current = id;
    const p = PAGES.find(x => x.id === id);
    $('#pageTitle').textContent = p.name;
    const body = $('#pageBody');
    body.innerHTML = '';
    const el = document.createElement('div');
    el.className = 'page';
    body.appendChild(el);
    RENDER[id](el);
    buildNav();
}

// ---------------- Power ----------------
function batteryColor(pct, charging) {
    if (pct >= 100 || (charging && pct >= 99)) return '#57a7ff';
    if (pct <= 20) return '#ff5a52';
    return '#7ed492';
}
function renderBattery() {
    const chip = $('#pillBattery');
    if (!battery) { chip.hidden = true; return; }
    const pct = Math.round(battery.level * 100);
    chip.hidden = false;
    chip.textContent = `${battery.charging ? '⚡' : ''}${pct}%`;
    chip.className = 'chip' + (pct <= 20 ? ' low' : pct >= 100 ? ' full' : '');
    if (!inSettings && current === 'power') showPage('power');
}
const RENDER = {};
RENDER.power = (el) => {
    if (!battery) {
        el.innerHTML = '<p class="muted" style="margin-top:30px;text-align:center">No battery detected on this machine.</p>';
        return;
    }
    const pct = Math.round(battery.level * 100);
    const color = batteryColor(pct, battery.charging);
    const state = battery.charging ? (pct >= 100 ? 'Fully charged' : 'Charging') : 'On battery';
    let mins = battery.charging ? battery.chargingTime : battery.dischargingTime;
    const time = isFinite(mins) && mins > 0 ? ` · about ${Math.floor(mins / 3600)}h ${Math.round((mins % 3600) / 60)}m ${battery.charging ? 'to full' : 'left'}` : '';
    el.innerHTML = `
      <div class="ring" style="background: conic-gradient(${color} ${pct * 3.6}deg, #2c2c33 0)">
        <div class="ringInner"><div><div class="big" style="font-size:38px">${pct}%</div>
        <div class="muted">${state}</div></div></div>
      </div>
      <p style="text-align:center" class="muted">${state}${time}</p>`;
};

// ---------------- Hardware ----------------
RENDER.hardware = (el) => {
    if (!lastSys) { el.innerHTML = '<p class="muted">Reading…</p>'; return; }
    const s = lastSys;
    const memPct = Math.round((1 - s.memFree / s.memTotal) * 100);
    const rows = [
        ['CPU', s.cpu, `${s.cpu}% · ${s.cpuModel}`],
        ['Memory', memPct, `${fmtBytes(s.memTotal - s.memFree)} of ${fmtBytes(s.memTotal)}`],
    ];
    if (s.disk) rows.push(['Disk C:', Math.round((1 - s.disk.free / s.disk.total) * 100),
        `${fmtBytes(s.disk.total - s.disk.free)} of ${fmtBytes(s.disk.total)}`]);
    el.innerHTML = rows.map(([n, pct, sub]) => `
      <div class="hwRow"><div class="row"><span>${n}</span><span class="muted">${sub}</span></div>
      <div class="bar"><div style="width:${pct}%"></div></div></div>`).join('') +
      `<p class="muted" style="margin-top:16px">Up ${Math.floor(s.uptime / 3600)}h ${Math.floor((s.uptime % 3600) / 60)}m · ${s.cores} cores</p>`;
};

// ---------------- Calendar ----------------
let calY, calM;
RENDER.calendar = (el) => {
    const now = new Date();
    if (calY === undefined) { calY = now.getFullYear(); calM = now.getMonth(); }
    const first = new Date(calY, calM, 1);
    const startDay = (first.getDay() + 6) % 7; // Monday first
    const days = new Date(calY, calM + 1, 0).getDate();
    const prevDays = new Date(calY, calM, 0).getDate();
    let cells = ['Mo', 'Tu', 'We', 'Th', 'Fr', 'Sa', 'Su'].map(d => `<div class="dow">${d}</div>`).join('');
    for (let i = 0; i < startDay; i++) cells += `<div class="day dim">${prevDays - startDay + 1 + i}</div>`;
    for (let d = 1; d <= days; d++) {
        const today = d === now.getDate() && calM === now.getMonth() && calY === now.getFullYear();
        cells += `<div class="day${today ? ' today' : ''}">${d}</div>`;
    }
    el.innerHTML = `
      <div class="calHead"><button id="calPrev">‹</button>
      <b>${first.toLocaleString(undefined, {month: 'long'})} ${calY}</b>
      <button id="calNext">›</button></div>
      <div class="calGrid">${cells}</div>`;
    el.querySelector('#calPrev').onclick = () => { calM--; if (calM < 0) { calM = 11; calY--; } showPage('calendar'); };
    el.querySelector('#calNext').onclick = () => { calM++; if (calM > 11) { calM = 0; calY++; } showPage('calendar'); };
};

// ---------------- Todo ----------------
RENDER.todo = (el) => {
    el.innerHTML = `
      <div class="row" style="margin-bottom:8px">
        <input type="text" id="todoIn" placeholder="Add a task and press Enter" style="flex:1">
      </div>
      <div id="todoList"></div>`;
    const list = el.querySelector('#todoList');
    const draw = () => {
        list.innerHTML = '';
        if (!store.todos.length) list.innerHTML = '<p class="muted" style="padding:10px 6px">Nothing here yet.</p>';
        store.todos.forEach((t, i) => {
            const row = document.createElement('div');
            row.className = 'todoItem' + (t.done ? ' done' : '');
            row.innerHTML = `<input type="checkbox" ${t.done ? 'checked' : ''}><span></span><button title="Delete">✕</button>`;
            row.querySelector('span').textContent = t.text;
            row.querySelector('input').onchange = (e) => { t.done = e.target.checked; save('todos'); draw(); };
            row.querySelector('button').onclick = () => { store.todos.splice(i, 1); save('todos'); draw(); };
            list.appendChild(row);
        });
    };
    el.querySelector('#todoIn').addEventListener('keydown', (e) => {
        if (e.key !== 'Enter') return;
        const v = e.target.value.trim();
        if (!v) return;
        store.todos.unshift({text: v, done: false});
        save('todos');
        e.target.value = '';
        draw();
    });
    draw();
};

// ---------------- Focus ----------------
const focus = {dur: 25 * 60, left: 25 * 60, running: false, endAt: 0, label: 'Focus'};
RENDER.focus = (el) => {
    el.innerHTML = `
      <div class="row" style="justify-content:center;gap:8px">
        ${[['Focus', 25], ['Break', 5], ['Long break', 15]].map(([n, m]) =>
          `<button class="chipBtn" data-m="${m}" data-n="${n}">${n} · ${m}m</button>`).join('')}
      </div>
      <div class="focusTime" id="fTime"></div>
      <p class="muted" style="text-align:center" id="fState"></p>
      <div class="row" style="justify-content:center;margin-top:18px;gap:10px">
        <button class="btn" id="fToggle">Start</button>
        <button class="btn ghost" id="fReset">Reset</button>
      </div>`;
    const time = el.querySelector('#fTime'), state = el.querySelector('#fState'), tog = el.querySelector('#fToggle');
    const draw = () => {
        const s = Math.max(0, Math.round(focus.left));
        time.textContent = `${String(Math.floor(s / 60)).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`;
        state.textContent = focus.running ? `${focus.label} — running` : focus.label;
        tog.textContent = focus.running ? 'Pause' : 'Start';
        el.querySelectorAll('.chipBtn').forEach(b =>
            b.classList.toggle('active', +b.dataset.m === focus.dur / 60 && b.dataset.n === focus.label));
    };
    el.querySelectorAll('.chipBtn').forEach(b => b.onclick = () => {
        focus.dur = +b.dataset.m * 60; focus.label = b.dataset.n;
        focus.left = focus.dur; focus.running = false; draw();
    });
    tog.onclick = () => {
        focus.running = !focus.running;
        if (focus.running) focus.endAt = Date.now() + focus.left * 1000;
        draw();
    };
    el.querySelector('#fReset').onclick = () => { focus.running = false; focus.left = focus.dur; draw(); };
    focus._draw = draw;
    draw();
};
setInterval(() => {
    if (!focus.running) return;
    focus.left = (focus.endAt - Date.now()) / 1000;
    if (focus.left <= 0) {
        focus.running = false;
        focus.left = focus.dur;
        window.isle.notify('Isle — Focus', `${focus.label} finished.`);
    }
    if (focus._draw && !inSettings && current === 'focus') focus._draw();
}, 250);

// ---------------- Water ----------------
RENDER.water = (el) => {
    if (store.water.day !== todayStr()) { store.water = {day: todayStr(), ml: 0, goal: store.water.goal || 2000}; save('water'); }
    const w = store.water;
    const pct = Math.min(100, Math.round(w.ml / w.goal * 100));
    el.innerHTML = `
      <div style="text-align:center;margin-top:22px">
        <div class="big">💧 ${w.ml}<span style="font-size:20px;color:#a9a9b3"> / ${w.goal} ml</span></div>
        <div class="bar" style="max-width:420px;margin:18px auto"><div style="width:${pct}%"></div></div>
        <div class="row" style="justify-content:center;gap:10px">
          <button class="btn" id="wAdd">+ 250 ml</button>
          <button class="btn ghost" id="wSub">− 250 ml</button>
          <button class="btn ghost" id="wReset">Reset</button>
        </div>
      </div>`;
    el.querySelector('#wAdd').onclick = () => { w.ml += 250; save('water'); showPage('water'); };
    el.querySelector('#wSub').onclick = () => { w.ml = Math.max(0, w.ml - 250); save('water'); showPage('water'); };
    el.querySelector('#wReset').onclick = () => { w.ml = 0; save('water'); showPage('water'); };
};

// ---------------- Shelf ----------------
const EXT_ICON = [[/\.(png|jpe?g|gif|webp|svg)$/i, '🖼️'], [/\.(mp3|wav|flac|ogg)$/i, '🎵'],
    [/\.(mp4|mkv|mov|avi)$/i, '🎬'], [/\.(zip|rar|7z)$/i, '🗜️'], [/\.(pdf|docx?|txt|md)$/i, '📄']];
RENDER.shelf = (el) => {
    el.innerHTML = `<div class="row" style="margin-bottom:12px">
        <button class="btn" id="sAdd">+ Add files</button>
        <span class="muted">Click a file to open it</span></div>
      <div class="grid" id="sGrid"></div>`;
    const grid = el.querySelector('#sGrid');
    const draw = () => {
        grid.innerHTML = store.shelf.length ? '' : '<p class="muted">The shelf is empty.</p>';
        for (const p of store.shelf) {
            const name = p.split(/[\\/]/).pop();
            const icon = (EXT_ICON.find(([re]) => re.test(name)) || [null, '📁'])[1];
            const t = document.createElement('div');
            t.className = 'tile';
            t.innerHTML = `<span class="x" title="Remove">✕</span><div class="ic">${icon}</div><div class="nm"></div>`;
            t.querySelector('.nm').textContent = name;
            t.title = p;
            t.onclick = () => window.isle.openPath(p);
            t.querySelector('.x').onclick = (e) => {
                e.stopPropagation();
                store.shelf = store.shelf.filter(x => x !== p);
                save('shelf'); draw();
            };
            grid.appendChild(t);
        }
    };
    el.querySelector('#sAdd').onclick = async () => {
        const picked = await window.isle.pickFiles();
        for (const p of picked) if (!store.shelf.includes(p)) store.shelf.push(p);
        save('shelf'); draw();
    };
    draw();
};

// ---------------- Wallpaper ----------------
let wpCache = null;
RENDER.wallpaper = (el) => {
    el.innerHTML = '<p class="muted">Scanning your Pictures and Windows wallpapers…</p>';
    const draw = (list) => {
        el.innerHTML = '<div class="grid" id="wGrid"></div>';
        const grid = el.querySelector('#wGrid');
        if (!list.length) { el.innerHTML = '<p class="muted">No wallpapers found.</p>'; return; }
        for (const p of list) {
            const t = document.createElement('div');
            t.className = 'tile' + (p === store.wpCurrent ? ' current' : '');
            const url = 'file:///' + p.replace(/\\/g, '/').split('/').map(encodeURIComponent).join('/');
            t.innerHTML = `<img src="${url}" loading="lazy"><div class="nm"></div>`;
            t.querySelector('.nm').textContent = p.split(/[\\/]/).pop();
            t.title = p;
            t.onclick = () => {
                window.isle.applyWallpaper(p);
                store.wpCurrent = p; save('wpCurrent');
                draw(list);
            };
            grid.appendChild(t);
        }
    };
    if (wpCache) draw(wpCache);
    else window.isle.wallpapers().then(list => { wpCache = list; draw(list); });
};

// ---------------- Settings ----------------
function showSettings() {
    inSettings = true;
    $('#pageTitle').textContent = 'Settings';
    const body = $('#pageBody');
    body.innerHTML = '';
    const el = document.createElement('div');
    el.className = 'page';
    body.appendChild(el);
    let html = '<p class="muted" style="margin-bottom:6px">Pages shown in the notch</p>';
    for (const p of PAGES) {
        html += `<div class="setRow"><div>${p.icon} ${p.name}</div>
          <button class="switch ${store.pages[p.id] !== false ? 'on' : ''}" data-page="${p.id}"></button></div>`;
    }
    html += `
      <div class="setRow"><div>Position<small>Where the notch sits on screen</small></div>
        <select id="setPos">
          <option value="top">Top centre</option><option value="right">Right middle</option><option value="left">Left middle</option>
        </select></div>
      <div class="setRow"><div>Size</div>
        <select id="setScale">
          <option value="0.9">Small</option><option value="1">Medium</option><option value="1.1">Large</option>
        </select></div>
      <div class="setRow"><div>Isle for Windows 0.1.0<small>Music, Lyrics and AI Usage arrive in the next builds — they need Windows media-session access.</small></div>
        <button class="btn ghost" id="setQuit">Quit Isle</button></div>`;
    el.innerHTML = html;
    el.querySelectorAll('.switch').forEach(sw => sw.onclick = () => {
        const id = sw.dataset.page;
        store.pages[id] = !(store.pages[id] !== false);
        save('pages');
        sw.classList.toggle('on', store.pages[id]);
        if (!visiblePages().some(p => p.id === current)) current = visiblePages()[0]?.id || 'power';
        buildNav();
    });
    const pos = el.querySelector('#setPos');
    pos.value = store.position;
    pos.onchange = () => { store.position = pos.value; save('position'); window.isle.setPosition(pos.value); };
    const sc = el.querySelector('#setScale');
    sc.value = String(store.scale);
    sc.onchange = () => { store.scale = +sc.value; save('scale'); document.body.style.zoom = store.scale; };
    el.querySelector('#setQuit').onclick = () => window.isle.quit();
    buildNav();
}

// ---------------- clocks & polling ----------------
function tickClock() {
    const d = new Date();
    $('#pillClock').textContent = d.toLocaleTimeString([], {hour: '2-digit', minute: '2-digit'});
    $('#pillDate').textContent = d.toLocaleDateString([], {weekday: 'short', day: 'numeric', month: 'short'});
}
setInterval(tickClock, 1000);
setInterval(async () => {
    lastSys = await window.isle.sysInfo();
    if (expanded && !inSettings && current === 'hardware') showPage('hardware');
}, 2000);

// ---------------- boot ----------------
(async () => {
    const saved = await window.isle.storeGet();
    store = {...store, ...saved, pages: saved.pages || {}, water: {...store.water, ...(saved.water || {})}};
    document.body.style.zoom = store.scale || 1;
    tickClock();
    lastSys = await window.isle.sysInfo();
    if (navigator.getBattery) {
        try {
            battery = await navigator.getBattery();
            battery.addEventListener('levelchange', renderBattery);
            battery.addEventListener('chargingchange', renderBattery);
        } catch { battery = null; }
    }
    renderBattery();
    current = visiblePages()[0]?.id || 'power';
    showPage(current);
})();
