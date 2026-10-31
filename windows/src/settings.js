// Isle Settings window — styled after the Nusa settings in Ram's
// reference video (icon-chip sidebar, grouped card rows, green switches,
// segmented controls). Edits the shared settings owned by the main
// process; every change applies to the notch instantly. Changes pushed
// from the tray or notch do NOT re-render this window mid-click — the
// local state is updated silently so sections never tear or flicker.
const main = document.querySelector('#main');
const side = document.querySelector('#side');
let S = null;
let section = 'general';

const SECTIONS = [
    {id: 'general', name: 'General', icon: '⚙', color: '#6a6a74'},
    {id: 'music', name: 'Music & lyrics', icon: '🎵', color: '#fc3c44'},
    {id: 'todo', name: 'To-do', icon: '✔', color: '#34c759'},
    {id: 'pomodoro', name: 'Pomodoro', icon: '🎯', color: '#ff6482'},
    {id: 'water', name: 'Water', icon: '💧', color: '#3d7bfd'},
    {id: 'shelf', name: 'Shelf', icon: '🗂', color: '#ff9f0a'},
    {id: 'about', name: 'About', icon: 'ℹ', color: '#6a6a74'},
];
const PAGE_NAMES = {music: 'Music', pomodoro: 'Pomodoro', water: 'Water',
    shelf: 'Shelf', todo: 'To-do', calendar: 'Calendar'};

const set = (path, value) => window.isle.settingsSet(path, value);
const sw = (on, fn) => {
    const b = document.createElement('button');
    b.className = 'sw' + (on ? ' on' : '');
    b.onclick = () => { const v = !b.classList.contains('on'); b.classList.toggle('on', v); fn(v); };
    return b;
};
const row = (title, sub, control) => {
    const r = document.createElement('div');
    r.className = 'srow';
    r.innerHTML = `<div class="tx"><div class="t"></div><div class="s"></div></div>`;
    r.querySelector('.t').textContent = title;
    r.querySelector('.s').textContent = sub || '';
    if (!sub) r.querySelector('.s').remove();
    if (control) r.appendChild(control);
    return r;
};
const num = (val, fn) => {
    const i = document.createElement('input');
    i.type = 'number'; i.className = 'num'; i.value = val; i.min = '1';
    i.onchange = () => { const v = parseInt(i.value, 10); if (v > 0) fn(v); };
    return i;
};

function renderSide() {
    side.innerHTML = '<div class="brand"><span class="dotR"></span>Isle</div>';
    for (const s of SECTIONS) {
        const b = document.createElement('button');
        b.className = 'nav' + (s.id === section ? ' active' : '');
        b.innerHTML = `<span class="ic" style="background:${s.color}">${s.icon}</span>${s.name}`;
        b.onclick = () => { section = s.id; render(); };
        side.appendChild(b);
    }
}

const RENDERERS = {
    general(m) {
        m.innerHTML = '<h1>General</h1>';
        const g1 = document.createElement('div');
        g1.innerHTML = '<div class="group">Startup</div>';
        g1.appendChild(row('Start with Windows', 'Isle opens on its own every time you sign in',
            sw(S.startWithWindows, v => set('startWithWindows', v))));
        m.appendChild(g1);
        const g2 = document.createElement('div');
        g2.innerHTML = '<div class="group">Notch</div>';
        const seg = document.createElement('div');
        seg.className = 'seg';
        const segOpts = [['auto', 'Auto'], ['100', '100%'], ['115', '115%'], ['130', '130%'], ['150', '150%']];
        for (const [val, label] of segOpts) {
            const b = document.createElement('button');
            b.textContent = label;
            b.className = String(S.size) === val ? 'on' : '';
            b.onclick = () => { set('size', val); seg.querySelectorAll('button').forEach(x => x.classList.toggle('on', x === b)); };
            seg.appendChild(b);
        }
        g2.appendChild(row('Size', 'Auto grows the notch on wide monitors', seg));
        g2.appendChild(row('Minimal notch', 'The closed notch becomes a thin black line. Alerts and hover still work',
            sw(S.minimal, v => set('minimal', v))));
        g2.appendChild(row('Isle mascot', 'When your PC is quiet, the clock steps aside and a pair of eyes appears',
            sw(S.mascot, v => set('mascot', v))));
        m.appendChild(g2);
        const g3 = document.createElement('div');
        g3.innerHTML = '<div class="group">Sounds</div>';
        g3.appendChild(row('Reminder sounds', 'Chimes for Pomodoro, To-do and Water',
            sw(S.sounds, v => set('sounds', v))));
        m.appendChild(g3);
        const g4 = document.createElement('div');
        g4.innerHTML = '<div class="group">Full screen</div>';
        g4.appendChild(row('Hide in full screen', 'Games, videos and slides that fill the screen',
            sw(S.hideFullscreen, v => set('hideFullscreen', v))));
        m.appendChild(g4);
        const g5 = document.createElement('div');
        g5.innerHTML = '<div class="group">Pages shown in the notch</div>';
        for (const [id, name] of Object.entries(PAGE_NAMES)) {
            g5.appendChild(row(name, '', sw(S.pages[id] !== false, v => set(`pages.${id}`, v))));
        }
        m.appendChild(g5);
    },
    music(m) {
        m.innerHTML = `<h1>Music &amp; lyrics</h1>
          <p class="note" style="margin-bottom:10px">Music comes from the Windows media sessions — Spotify, Apple Music,
          Windows Media Player and browsers (including YouTube) all appear automatically. Lyrics are fetched
          from LRCLIB; only the song title, artist and length are sent.</p>
          <div class="group">Lyrics</div>`;
        m.appendChild(row('Show lyrics', 'A second pane beside the player shows the synced line as it is sung',
            sw(S.lyricsPill, v => set('lyricsPill', v))));
    },
    todo(m) {
        m.innerHTML = '<h1>To-do</h1><div class="group">Reminders</div>';
        m.appendChild(row('Task reminders', "When a task's time arrives, the notch opens with a Time's up! banner and snooze",
            sw(S.todoRemind !== false, v => set('todoRemind', v))));
    },
    pomodoro(m) {
        m.innerHTML = '<h1>Pomodoro</h1><div class="group">Session lengths, in minutes</div>';
        m.appendChild(row('Focus', '', num(S.pomo.focus, v => set('pomo.focus', v))));
        m.appendChild(row('Short break', '', num(S.pomo.short, v => set('pomo.short', v))));
        m.appendChild(row('Long break', '', num(S.pomo.long, v => set('pomo.long', v))));
    },
    water(m) {
        m.innerHTML = '<h1>Water</h1><div class="group">Your daily goal and the size of one drink</div>';
        m.appendChild(row('Daily goal (ml)', '', num(S.water.goal, v => set('water.goal', v))));
        m.appendChild(row('Bottle / one drink (ml)', '', num(S.water.bottle, v => set('water.bottle', v))));
        const g = document.createElement('div');
        g.innerHTML = '<div class="group">Reminders</div>';
        g.appendChild(row('Reminders', 'A gentle nudge once an hour, 8:00–22:00, while you are behind your goal',
            sw(S.water.remind, v => set('water.remind', v))));
        m.appendChild(g);
    },
    shelf(m) {
        m.innerHTML = `<h1>Shelf</h1>
          <p class="note">Drag files onto the notch to keep them close, drag them out into a chat, an email or a
          folder, and click to open. Text cards copy their text when clicked. The shelf remembers where your
          files live — it never copies them.</p>`;
        const g = document.createElement('div');
        g.innerHTML = '<div class="group" style="margin-top:14px">Contents</div>';
        const btn = document.createElement('button');
        btn.className = 'plainBtn danger';
        btn.textContent = 'Clear all';
        btn.onclick = () => window.isle.storeSet('shelf', []);
        g.appendChild(row('Empty the shelf', 'Removes every pinned file and text card (the files themselves stay put)', btn));
        m.appendChild(g);
    },
    about(m) {
        m.innerHTML = `<h1>About</h1>
          <p class="note">Isle for Windows 0.3.1 — a living notch for the top of your screen, matched to the
          Nusa app. Part of the Isle project: the GNOME version lives on the main branch of this repository.
          Tasks, water logs, shelf items and settings are stored only on this PC, in your user folder.</p>`;
        const g = document.createElement('div');
        g.innerHTML = '<div class="group" style="margin-top:14px">App</div>';
        const btn = document.createElement('button');
        btn.className = 'plainBtn';
        btn.textContent = 'Quit Isle';
        btn.onclick = () => window.isle.quit();
        g.appendChild(row('Close Isle completely', '', btn));
        m.appendChild(g);
    },
};

function render() {
    renderSide();
    RENDERERS[section](main);
}

(async () => {
    S = await window.isle.settingsGet();
    window.isle.onSettings((s) => { S = s; });  // silent sync — no re-render
    render();
})();
