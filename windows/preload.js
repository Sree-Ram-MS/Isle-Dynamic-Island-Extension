// The only bridge between the notch UI and the system.
const {contextBridge, ipcRenderer} = require('electron');

contextBridge.exposeInMainWorld('isle', {
    storeGet: () => ipcRenderer.invoke('store:get'),
    storeSet: (key, value) => ipcRenderer.invoke('store:set', key, value),
    sysInfo: () => ipcRenderer.invoke('sys:info'),
    wallpapers: () => ipcRenderer.invoke('wp:list'),
    applyWallpaper: (p) => ipcRenderer.invoke('wp:apply', p),
    pickFiles: () => ipcRenderer.invoke('shelf:pick'),
    openPath: (p) => ipcRenderer.invoke('shell:open', p),
    notify: (title, body) => ipcRenderer.invoke('notify', title, body),
    setExpanded: (expanded) => ipcRenderer.invoke('win:expand', expanded),
    setPosition: (pos) => ipcRenderer.invoke('win:position', pos),
    quit: () => ipcRenderer.invoke('app:quit'),
});
