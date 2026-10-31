// Isle Settings window — edits the shared settings owned by the main
// process; every change applies to the notch instantly.
const main = document.querySelector('#main');
let S = null;
let section = 'general';

const PAGES = [
    {id: 'music', icon: '🎵', name: 'Music'},
    {id: 'pomodoro', icon: '🎯', name: 'Pomodoro'},
    {id: 'water', icon: '💧', name: 'Water'},
    {id: 'shelf', icon: '🗂️', name: 'Shelf'},
    {id: 'todo', icon: '✔️', name: 'Todo'},
    {id: 'calendar', icon: '📅', name: 'Calendar'},
];

function sw(label, sub, on, fn) {
    return `<div class="row"><div>${label}${sub ? `<small>${sub}</small>` : ''}</div>
      <button class="switch ${on ? 'on' : ''}" data-fn="${fn}"></button></div>`;
}
const FNS = {
    startWithWindows: () => set({startWithWindows: !S.startWithWindows}),
    mascot: () => set({mascot: !S.mascot}),
    minimal: () => set({minimal: !S.minimal}),
    hideFullscreen: () => set({hideFullscreen: !S.hideFullscreen}),
    sounds: () => set({sounds: !S.sounds}),
    lyricsPill: () => set({lyricsPill: !S.lyricsPill}),
    waterRemind: () => set({water: {...S.water, remind: !S.water.remind}}),
};
function set(patch) {
    window.isle.settingsSet(patch);
}

const SECTIONS = {
    general() {
        let h = `<h2>General</h2><p class="sub">How Isle behaves on your screen. Changes apply instantly.</p>`;
        h += sw('Start with Windows', 'Isle opens on its own every time you sign in', S.startWithWindows, 'startWithWindows');
        h += `<div class="row"><div>Notch size<small>Auto grows a step on wide monitors</small></div>
          <select id="sizeSel">
            ${[['auto', 'Auto'], ['100', '100%'], ['115', '115%'], ['130', '130%'], ['150', '150%']]
              .map(([v, n]) => `<option value="${v}" ${S.size === v ? 'selected' : ''}>${n}</option>`).join('')}
          </select></div>`;
        h += sw('Minimal notch', 'The closed notch becomes a thin black line. Alerts and hover still work', S.minimal, 'minimal');
        h += sw('Isle mascot', 'When your PC is quiet, the clock steps aside and a pair of eyes appears', S.mascot, 'mascot');
        h += sw('Hide in full screen', 'Isle steps aside for full-screen games, videos and slides', S.hideFullscreen, 'hideFullscreen');
        h += sw('Reminder sounds', 'A soft chime when a Pomodoro session ends', S.sounds, 'sounds');
        h += `<p class="sub" style="margin-top:20px">Pages shown in the notch</p>`;
        for (const p of PAGES) {
            h += `<div class="row"><div>${p.icon} ${p.name}</div>
              <button class="switch ${S.pages[p.id] !== false ? 'on' : ''}" data-page="${p.id}"></button></div>`;
        }
        return h;
    },
    music() {
        return `<h2>Music &amp; lyrics</h2>
          <p class="sub">Music comes from the Windows media sessions — Spotify, Apple Music,
          Windows Media Player and browsers (including YouTube) all appear automatically.
          Lyrics are fetched from LRCLIB; only the song title, artist and length are sent.</p>
          ${sw('Lyrics split pill', 'A second pill under the notch shows the synced line as it is sung', S.lyricsPill, 'lyricsPill')}`;
    },
    pomodoro() {
        return `<h2>Pomodoro</h2><p class="sub">Session lengths in minutes.</p>
          ${[['focus', 'Focus'], ['short', 'Short break'], ['long', 'Long break']].map(([k, n]) => `
          <div class="row"><div>${n}</div>
            <input type="number" min="1" max="180" data-pomo="${k}" value="${S.pomo[k]}"></div>`).join('')}`;
    },
    water() {
        return `<h2>Water</h2><p class="sub">Your daily goal and the size of one Drink.</p>
          <div class="row"><div>Daily goal (ml)</div>
            <input type="number" min="250" step="50" data-water="goal" value="${S.water.goal}"></div>
          <div class="row"><div>Bottle / one drink (ml)</div>
            <input type="number" min="50" step="10" data-water="bottle" value="${S.water.bottle}"></div>
          ${sw('Reminders', 'A gentle nudge once an hour, 8:00–22:00, while you are behind your goal', S.water.remind, 'waterRemind')}`;
    },
    shelf() {
        return `<h2>Shelf</h2>
          <p class="sub">Drag files onto the notch to keep them close, drag them out into a chat,
          an email or a folder, and click to open. Text cards copy their text when clicked.
          The shelf remembers where your files live — it never copies them.</p>
          <div class="row"><div>Empty the shelf</div>
            <button class="btn ghost" id="clearShelf">Clear all</button></div>`;
    },
    about() {
        return `<h2>About</h2>
          <p class="sub">Isle for Windows 0.2.0 — a living notch for the top of your screen,
          modelled on the Nusa app. Part of the Isle project: the GNOME version lives on the
          main branch of this repository. Tasks, water logs, shelf items and settings are stored
          only on this PC, in your user folder.</p>
          <div class="row"><div>Close Isle completely</div>
            <button class="btn ghost" id="quitBtn">Quit Isle</button></div>`;
    },
};

function render() {
    main.innerHTML = SECTIONS[section]();
    main.querySelectorAll('.switch[data-fn]').forEach(b =>
        b.onclick = () => FNS[b.dataset.fn]());
    main.querySelectorAll('.switch[data-page]').forEach(b =>
        b.onclick = () => set({pages: {...S.pages, [b.dataset.page]: !(S.pages[b.dataset.page] !== false)}}));
    const size = main.querySelector('#sizeSel');
    if (size) size.onchange = () => set({size: size.value});
    main.querySelectorAll('[data-pomo]').forEach(inp =>
        inp.onchange = () => set({pomo: {...S.pomo, [inp.dataset.pomo]: Math.max(1, +inp.value || 1)}}));
    main.querySelectorAll('[data-water]').forEach(inp =>
        inp.onchange = () => set({water: {...S.water, [inp.dataset.water]: Math.max(10, +inp.value || 10)}}));
    const clear = main.querySelector('#clearShelf');
    if (clear) clear.onclick = async () => { await window.isle.storeSet('shelf', []); clear.textContent = 'Cleared ✓'; };
    const quit = main.querySelector('#quitBtn');
    if (quit) quit.onclick = () => window.isle.quit();
}

document.querySelectorAll('#side button').forEach(b => b.onclick = () => {
    section = b.dataset.sec;
    document.querySelectorAll('#side button').forEach(x => x.classList.toggle('active', x === b));
    render();
});

(async () => {
    S = await window.isle.settingsGet();
    window.isle.onSettings((s) => { S = s; render(); });
    render();
})();
