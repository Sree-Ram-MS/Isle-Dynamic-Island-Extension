// Isle for Windows — main process.
// A floating notch pill (always on top, no taskbar button) that expands to
// the same pages as the GNOME extension: Power, Hardware, Calendar, Todo,
// Focus, Water, Shelf, Wallpaper — plus a Settings view. All data is real:
// battery from Chromium's Battery API in the renderer, CPU/RAM/disk from
// Node's os/fs in here, wallpapers from the real Windows wallpaper folders,
// applied through user32 SystemParametersInfo.

const {app, BrowserWindow, ipcMain, dialog, shell, Notification, screen} = require('electron');
const fs = require('fs');
const path = require('path');
const os = require('os');
const {execFile} = require('child_process');

const COLLAPSED = {width: 380, height: 56};
const EXPANDED = {width: 760, height: 540};

let win = null;
let position = 'top'; // 'top' | 'right' | 'left' — read from the store at boot

// ---------- tiny JSON store in userData ----------
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
function writeStore(data) {
    try {
        fs.writeFileSync(storeFile(), JSON.stringify(data));
    } catch { /* read-only profile: run unpersisted */ }
}

// ---------- window placement ----------
function boundsFor(expanded) {
    const size = expanded ? EXPANDED : COLLAPSED;
    const area = screen.getPrimaryDisplay().workArea;
    let x, y;
    if (position === 'right') {
        x = area.x + area.width - size.width - 14;
        y = area.y + Math.round((area.height - size.height) / 2);
    } else if (position === 'left') {
        x = area.x + 14;
        y = area.y + Math.round((area.height - size.height) / 2);
    } else {
        x = area.x + Math.round((area.width - size.width) / 2);
        y = area.y + 10;
    }
    return {x, y, width: size.width, height: size.height};
}

function createWindow() {
    win = new BrowserWindow({
        ...boundsFor(false),
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
    win.setAlwaysOnTop(true, 'screen-saver');
    win.loadFile(path.join(__dirname, 'src', 'index.html'));
    win.once('ready-to-show', () => win.show());
}

// ---------- system info (CPU sampled between calls) ----------
let lastCpu = null;
function cpuPercent() {
    const cpus = os.cpus();
    let idle = 0, total = 0;
    for (const c of cpus) {
        idle += c.times.idle;
        total += c.times.user + c.times.nice + c.times.sys + c.times.idle + c.times.irq;
    }
    if (!lastCpu) {
        lastCpu = {idle, total};
        return 0;
    }
    const dIdle = idle - lastCpu.idle;
    const dTotal = total - lastCpu.total;
    lastCpu = {idle, total};
    if (dTotal <= 0) return 0;
    return Math.max(0, Math.min(100, Math.round((1 - dIdle / dTotal) * 100)));
}

async function sysInfo() {
    let disk = null;
    try {
        const s = await fs.promises.statfs('C:\\');
        disk = {total: s.blocks * s.bsize, free: s.bfree * s.bsize};
    } catch { /* no C: — leave null, the page hides the row */ }
    return {
        cpu: cpuPercent(),
        cpuModel: os.cpus()[0]?.model ?? '',
        cores: os.cpus().length,
        memTotal: os.totalmem(),
        memFree: os.freemem(),
        uptime: os.uptime(),
        disk,
    };
}

// ---------- wallpapers ----------
function scanWallpapers() {
    const roots = [
        path.join(os.homedir(), 'Pictures'),
        'C:\\Windows\\Web\\Wallpaper',
        'C:\\Windows\\Web\\4K\\Wallpaper',
    ];
    const out = [];
    const walk = (dir, depth) => {
        if (depth > 2 || out.length >= 80) return;
        let entries;
        try {
            entries = fs.readdirSync(dir, {withFileTypes: true});
        } catch {
            return;
        }
        for (const e of entries) {
            if (e.isDirectory()) walk(path.join(dir, e.name), depth + 1);
            else if (/\.(jpe?g|png)$/i.test(e.name)) out.push(path.join(dir, e.name));
            if (out.length >= 80) return;
        }
    };
    for (const r of roots) walk(r, 0);
    return out;
}

function applyWallpaper(file) {
    const safe = file.replace(/'/g, "''");
    const ps = "Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; " +
        'public class IsleW { [DllImport("user32.dll", SetLastError=true)] ' +
        "public static extern bool SystemParametersInfo(int a, int b, string c, int d); }'; " +
        `[IsleW]::SystemParametersInfo(20, 0, '${safe}', 3)`;
    execFile('powershell.exe', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-Command', ps], () => {});
}

// ---------- IPC ----------
ipcMain.handle('store:get', () => readStore());
ipcMain.handle('store:set', (_e, key, value) => {
    const data = readStore();
    data[key] = value;
    writeStore(data);
});
ipcMain.handle('sys:info', () => sysInfo());
ipcMain.handle('wp:list', () => scanWallpapers());
ipcMain.handle('wp:apply', (_e, file) => applyWallpaper(file));
ipcMain.handle('shelf:pick', async () => {
    const r = await dialog.showOpenDialog(win, {
        properties: ['openFile', 'multiSelections'],
        title: 'Add files to the Shelf',
    });
    return r.canceled ? [] : r.filePaths;
});
ipcMain.handle('shell:open', (_e, p) => shell.openPath(p));
ipcMain.handle('notify', (_e, title, body) => {
    if (Notification.isSupported()) new Notification({title, body}).show();
});
ipcMain.handle('win:expand', (_e, expanded) => {
    if (win) win.setBounds(boundsFor(expanded), true);
});
ipcMain.handle('win:position', (_e, pos) => {
    position = pos;
    if (win) win.setBounds(boundsFor(false), true);
});
ipcMain.handle('app:quit', () => app.quit());

// ---------- boot ----------
if (!app.requestSingleInstanceLock()) {
    app.quit();
} else {
    app.whenReady().then(() => {
        position = readStore().position || 'top';
        createWindow();
    });
    app.on('window-all-closed', () => app.quit());
}
