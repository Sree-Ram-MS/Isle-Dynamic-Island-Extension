// The only bridge between the notch UI and the system.
const {contextBridge, ipcRenderer, webUtils} = require('electron');

contextBridge.exposeInMainWorld('isle', {
    // renderer-owned data (todos, shelf, water log)
    storeGet: () => ipcRenderer.invoke('store:get'),
    storeSet: (key, value) => ipcRenderer.invoke('store:set', key, value),
    // shared settings (owned by the main process)
    settingsGet: () => ipcRenderer.invoke('settings:get'),
    settingsSet: (patch) => ipcRenderer.invoke('settings:set', patch),
    onSettings: (cb) => ipcRenderer.on('settings:changed', (_e, s) => cb(s)),
    openSettings: () => ipcRenderer.invoke('settings:open'),
    // music via the Windows media sessions
    musicInfo: () => ipcRenderer.invoke('music:info'),
    musicCmd: (cmd, arg) => ipcRenderer.invoke('music:cmd', cmd, arg),
    volGet: () => ipcRenderer.invoke('vol:get'),
    volSet: (v) => ipcRenderer.invoke('vol:set', v),
    // files
    stat: (p) => ipcRenderer.invoke('fs:stat', p),
    openPath: (p) => ipcRenderer.invoke('shell:open', p),
    pathForFile: (file) => webUtils.getPathForFile(file),
    dragStart: (files) => ipcRenderer.send('dragstart', files),
    // window + app
    setMode: (mode) => ipcRenderer.invoke('win:mode', mode),
    notify: (title, body) => ipcRenderer.invoke('notify', title, body),
    quit: () => ipcRenderer.invoke('app:quit'),
});
