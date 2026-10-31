// Isle for Windows — main process.
// A notch attached to the top edge of the screen, modelled on the Nusa
// app: hover to open, pages for Music (real Windows media sessions via
// scripts/gsm.ps1), Pomodoro, Water, Shelf, Todo and Calendar, a mascot
// that appears when the PC is idle, a tray icon and a Settings window.
//
// Settings live here (single source of truth, shared by the notch, the
// tray menu and the Settings window). Todos / shelf / water data are
// managed by the renderer through the same JSON store file.

const {app, BrowserWindow, Tray, Menu, ipcMain, shell, Notification, screen} = require('electron');
const fs = require('fs');
const path = require('path');
const {execFile} = require('child_process');

// ---------- store ----------
function storeFile() {
    return path.join(app.getPath('userData'), 'isle-data.json');
}
function readStore() {
    try {
        return JSON.parse(fs.readFileSync(storeFile(), 'utf8'));
    } catch {
        return {};
    }
}
function writeKey(key, value) {
    const data = readStore();
    data[key] = value;
    try {
        fs.writeFileSync(storeFile(), JSON.stringify(data));
    } catch { /* read-only profile */ }
}

// ---------- settings ----------
const DEFAULT_SETTINGS = {
    startWithWindows: false,
    mascot: true,
    minimal: false,
    size: 'auto', // auto | 100 | 115 | 130 | 150
    hideFullscreen: true,
    sounds: true,
    lyricsPill: true,
    todoRemind: true,
    pages: {music: true, pomodoro: true, water: true, shelf: true, todo: true, calendar: true},
    pomo: {focus: 25, short: 5, long: 15},
    water: {goal: 1500, bottle: 250, remind: true},
};
let settings = {...DEFAULT_SETTINGS, ...(readStore().settings || {})};
settings.pages = {...DEFAULT_SETTINGS.pages, ...settings.pages};
settings.pomo = {...DEFAULT_SETTINGS.pomo, ...settings.pomo};
settings.water = {...DEFAULT_SETTINGS.water, ...settings.water};

function scaleFactor() {
    if (settings.size !== 'auto') return parseInt(settings.size, 10) / 100;
    return screen.getPrimaryDisplay().bounds.width >= 1920 ? 1.15 : 1.0;
}

// ---------- notch window ----------
// The renderer decides the exact size for every state (the original
// notch hugs its content: a ~216px clock pill, a 544px card whose
// height fits the page) and the main process just scales and centres.
let notch = null;
let settingsWin = null;
let tray = null;
let lastSize = {width: 216, height: 42};
let hiddenByFullscreen = false;

function boundsFor(size) {
    const s = scaleFactor();
    const width = Math.round(size.width * s);
    const height = Math.round(size.height * s);
    const b = screen.getPrimaryDisplay().bounds;
    return {x: b.x + Math.round((b.width - width) / 2), y: b.y, width, height};
}
function applySize() {
    if (notch) notch.setBounds(boundsFor(lastSize));
}

function createNotch() {
    notch = new BrowserWindow({
        ...boundsFor(lastSize),
        transparent: true,
        frame: false,
        resizable: false,
        movable: false,
        alwaysOnTop: true,
        skipTaskbar: true,
        show: false,
        webPreferences: {
            preload: path.join(__dirname, 'preload.js'),
            contextIsolation: true,
            nodeIntegration: false,
        },
    });
    notch.setAlwaysOnTop(true, 'screen-saver');
    notch.loadFile(path.join(__dirname, 'src', 'index.html'));
    notch.once('ready-to-show', () => notch.show());
}

// ---------- settings window ----------
function openSettings() {
    if (settingsWin) {
        settingsWin.focus();
        return;
    }
    settingsWin = new BrowserWindow({
        width: 780,
        height: 560,
        title: 'Isle Settings',
        autoHideMenuBar: true,
        backgroundColor: '#17171a',
        webPreferences: {
            preload: path.join(__dirname, 'preload.js'),
            contextIsolation: true,
            nodeIntegration: false,
        },
    });
    settingsWin.loadFile(path.join(__dirname, 'src', 'settings.html'));
    settingsWin.on('closed', () => { settingsWin = null; });
}

// ---------- tray ----------
function buildTray() {
    if (!tray) {
        tray = new Tray(path.join(__dirname, 'assets', 'icon.png'));
        tray.setToolTip('Isle');
        tray.on('click', openSettings);
    }
    const check = (key) => settings[key];
    tray.setContextMenu(Menu.buildFromTemplate([
        {label: 'Isle Settings…', click: openSettings},
        {type: 'separator'},
        {label: 'Start with Windows', type: 'checkbox', checked: check('startWithWindows'),
            click: (item) => setSettings({startWithWindows: item.checked})},
        {label: 'Mascot', type: 'checkbox', checked: check('mascot'),
            click: (item) => setSettings({mascot: item.checked})},
        {label: 'Minimal notch', type: 'checkbox', checked: check('minimal'),
            click: (item) => setSettings({minimal: item.checked})},
        {type: 'separator'},
        {label: 'Exit Isle', click: () => app.quit()},
    ]));
}

// ---------- settings plumbing ----------
function applySettingsSideEffects() {
    const exe = process.env.PORTABLE_EXECUTABLE_FILE || process.execPath;
    app.setLoginItemSettings({openAtLogin: !!settings.startWithWindows, path: exe});
    applySize();
    buildTray();
}
function setSettings(patch) {
    settings = {...settings, ...patch};
    if (patch.pages) settings.pages = {...DEFAULT_SETTINGS.pages, ...settings.pages};
    if (patch.pomo) settings.pomo = {...DEFAULT_SETTINGS.pomo, ...settings.pomo};
    if (patch.water) settings.water = {...DEFAULT_SETTINGS.water, ...settings.water};
    writeKey('settings', settings);
    applySettingsSideEffects();
    for (const w of [notch, settingsWin]) {
        if (w && !w.isDestroyed()) {
            w.webContents.send('settings:changed', {...settings, scale: scaleFactor()});
        }
    }
}

// ---------- PowerShell bridges (media sessions / volume / fullscreen) ----------
function runPs(script, args) {
    return new Promise((resolve) => {
        execFile('powershell.exe',
            ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', path.join(__dirname, 'scripts', script), ...args],
            {timeout: 8000, maxBuffer: 8 * 1024 * 1024, windowsHide: true},
            (err, stdout) => {
                if (err && !stdout) return resolve(null);
                try {
                    resolve(JSON.parse(String(stdout).trim().split('\n').pop()));
                } catch {
                    resolve(null);
                }
            });
    });
}

let musicInFlight = null;
let musicCache = {key: null, art: '', artType: '', info: null};
async function musicInfo() {
    if (musicInFlight) return musicCache.info;
    musicInFlight = (async () => {
        const info = await runPs('gsm.ps1', ['-Mode', 'info']);
        if (info && info.hasSession) {
            const key = `${info.title}|${info.artist}`;
            if (key !== musicCache.key) {
                musicCache = {key, art: '', artType: '', info};
                const withArt = await runPs('gsm.ps1', ['-Mode', 'info', '-Art', '1']);
                if (withArt && withArt.art && `${withArt.title}|${withArt.artist}` === key) {
                    musicCache.art = withArt.art;
                    musicCache.artType = withArt.artType || '';
                }
            } else {
                musicCache.info = info;
            }
            if (musicCache.info) {
                musicCache.info.art = musicCache.art;
                musicCache.info.artType = musicCache.artType;
            }
        } else {
            musicCache = {key: null, art: '', artType: '', info: info || {hasSession: false}};
        }
        musicInFlight = null;
        return musicCache.info;
    })();
    return musicInFlight;
}

// ---------- fullscreen watcher (games/videos: step aside) ----------
setInterval(async () => {
    if (!notch || notch.isDestroyed()) return;
    if (!settings.hideFullscreen) {
        if (hiddenByFullscreen) { notch.show(); hiddenByFullscreen = false; }
        return;
    }
    const r = await runPs('win.ps1', ['-Mode', 'fg']);
    if (!r || typeof r.fullscreen !== 'boolean') return;
    if (r.fullscreen && !hiddenByFullscreen) {
        notch.hide();
        hiddenByFullscreen = true;
    } else if (!r.fullscreen && hiddenByFullscreen) {
        notch.show();
        hiddenByFullscreen = false;
    }
}, 3000);

// ---------- IPC ----------
ipcMain.handle('store:get', () => readStore());
ipcMain.handle('store:set', (_e, key, value) => writeKey(key, value));
ipcMain.handle('settings:get', () => ({...settings, scale: scaleFactor()}));
ipcMain.handle('settings:set', (_e, patch) => setSettings(patch));
ipcMain.handle('settings:open', () => openSettings());
ipcMain.handle('music:info', () => musicInfo());
ipcMain.handle('music:cmd', (_e, cmd, arg) =>
    runPs('gsm.ps1', ['-Mode', 'cmd', '-Cmd', String(cmd), '-Arg', String(arg || 0)]));
ipcMain.handle('vol:get', () => runPs('win.ps1', ['-Mode', 'volget']));
ipcMain.handle('vol:set', (_e, v) => runPs('win.ps1', ['-Mode', 'volset', '-Arg', String(v)]));
ipcMain.handle('fs:stat', (_e, p) => {
    try {
        const s = fs.statSync(p);
        return {exists: true, size: s.size, isDir: s.isDirectory()};
    } catch {
        return {exists: false, size: 0, isDir: false};
    }
});
ipcMain.handle('shell:open', (_e, p) => shell.openPath(p));
ipcMain.handle('notify', (_e, title, body) => {
    if (Notification.isSupported()) new Notification({title, body}).show();
});
ipcMain.handle('win:size', (_e, size) => {
    if (size && size.width >= 100 && size.height >= 10) {
        lastSize = {width: Math.round(size.width), height: Math.round(size.height)};
        applySize();
    }
});
ipcMain.handle('app:quit', () => app.quit());
ipcMain.on('dragstart', (event, files) => {
    const list = (files || []).filter(f => typeof f === 'string' && fs.existsSync(f));
    if (list.length) {
        event.sender.startDrag({files: list, icon: path.join(__dirname, 'assets', 'icon.png')});
    }
});

// ---------- boot ----------
if (!app.requestSingleInstanceLock()) {
    app.quit();
} else {
    app.whenReady().then(() => {
        createNotch();
        applySettingsSideEffects();
    });
    // The app lives in the tray; open windows closing never quits it.
    app.on('window-all-closed', () => {});
}
