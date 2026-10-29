import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

import {Emitter, todayKey, dayKeyOffset, dateKeyToMs, minutesNow, minutesOf, nextOccurrence, startOfToday} from './util.js';

Gio._promisify(Gio.DBusConnection.prototype, 'call', 'call_finish');

function getSeat() {
    try {
        return global.stage.context.get_backend().get_default_seat();
    } catch (_e) {
        return Clutter.get_default_backend().get_default_seat();
    }
}

export function playChime() {
    try {
        global.display.get_sound_player().play_from_theme('complete', 'Nusa Notch', null);
    } catch (_e) { /* no sound theme */ }
}

// ---------------------------------------------------------------- focus timer
export class FocusService {
    constructor(settings, timers) {
        this._s = settings;
        this._timers = timers;
        this.changed = new Emitter();
        this.onFinished = null;
        this.mode = 'focus';
        this.running = false;
        this.sessions = 0;
        this.remaining = this._len('focus');
        this._tick = 0;
    }

    _len(mode) {
        return this._s.get_int(mode === 'focus' ? 'focus-minutes' : 'break-minutes') * 60;
    }

    toggle() {
        if (this.running) this.pause();
        else this.start();
    }

    start() {
        if (this.running) return;
        this.running = true;
        this._tick = this._timers.add(1000, () => this._step());
        this.changed.emit();
    }

    pause() {
        this.running = false;
        this._timers.clear(this._tick);
        this._tick = 0;
        this.changed.emit();
    }

    reset() {
        this.pause();
        this.remaining = this._len(this.mode);
        this.changed.emit();
    }

    skip() {
        this._switchMode(false);
    }

    // jump straight to 'focus' or 'break' (resets the timer)
    setMode(mode) {
        this.pause();
        this.mode = mode;
        this.remaining = this._len(mode);
        this.changed.emit();
    }

    get total() {
        return this._len(this.mode);
    }

    refreshLengths() {
        if (!this.running) this.remaining = this._len(this.mode);
        this.changed.emit();
    }

    _step() {
        if (!this.running) return false;
        this.remaining -= 1;
        if (this.remaining <= 0) {
            this._switchMode(true);
            return false;
        }
        this.changed.emit();
        return true;
    }

    _switchMode(finished) {
        const wasFocus = this.mode === 'focus';
        if (finished && wasFocus) this.sessions += 1;
        this.pause();
        this.mode = wasFocus ? 'break' : 'focus';
        this.remaining = this._len(this.mode);
        this.changed.emit();
        if (finished) {
            playChime();
            this.onFinished?.(wasFocus);
        }
    }

    destroy() {
        this._timers.clear(this._tick);
    }
}

// ---------------------------------------------------------------- water
export class WaterService {
    constructor(settings, store, timers) {
        this._s = settings;
        this._store = store;
        this._timers = timers;
        this.changed = new Emitter();
        this.onReminder = null;
        this._lastRemind = Date.now();
        this._id = this._timers.add(60 * 1000, () => {
            this._check();
            return true;
        });
    }

    get goal() { return this._s.get_int('water-goal-ml'); }
    get bottle() { return this._s.get_int('water-bottle-ml'); }
    get startMin() { return this._s.get_int('water-start-min'); }
    get endMin() { return this._s.get_int('water-end-min'); }
    get interval() { return this._s.get_int('water-interval-min'); }

    // how many reminders fit into the reminder hours, the sip each one stands for, and bottle refills
    get reminderCount() { return Math.max(1, Math.floor((this.endMin - this.startMin) / this.interval)); }
    get sipMl() { return Math.max(1, Math.round(this.goal / this.reminderCount)); }
    get sipsPerBottle() { return Math.max(1, Math.round(this.bottle / this.sipMl)); }
    get refills() { return Math.max(0, Math.ceil(this.goal / this.bottle) - 1); }

    resetToday() {
        delete this._store.data.water[todayKey()];
        this._store.save();
        this.changed.emit();
    }

    _day(key) {
        return this._store.data.water[key] ?? [];
    }

    total(key = todayKey()) {
        return this._day(key).reduce((a, e) => a + e.ml, 0);
    }

    drink(ml = this.bottle) {
        const key = todayKey();
        (this._store.data.water[key] ??= []).push({t: Date.now(), ml});
        this._lastRemind = Date.now();
        this._prune();
        this._store.save();
        this.changed.emit();
    }

    undo() {
        const day = this._store.data.water[todayKey()];
        if (day?.length) {
            day.pop();
            this._store.save();
            this.changed.emit();
        }
    }

    // last 7 days, oldest first: [{key, ml, daysAgo}]
    week() {
        const out = [];
        for (let i = 6; i >= 0; i--) {
            const key = dayKeyOffset(i);
            out.push({key, ml: this.total(key), daysAgo: i});
        }
        return out;
    }

    _prune() {
        const keep = new Set();
        for (let i = 0; i < 60; i++) keep.add(dayKeyOffset(i));
        for (const k of Object.keys(this._store.data.water))
            if (!keep.has(k)) delete this._store.data.water[k];
    }

    _check() {
        if (!this._s.get_boolean('water-reminders')) return;
        const now = minutesNow();
        if (now < this.startMin || now >= this.endMin) return;
        if (this.total() >= this.goal) return;
        const every = this.interval * 60 * 1000;
        if (Date.now() - this._lastRemind < every) return;
        this._lastRemind = Date.now();
        this.onReminder?.();
    }

    destroy() {
        this._timers.clear(this._id);
    }
}

// ---------------------------------------------------------------- to-do
export const TASK_COLORS = ['#ff6b6b', '#ffb454', '#ffd93d', '#6bcB77', '#4fb7ff', '#b583ff'];

export class TodoService {
    constructor(store, timers) {
        this._store = store;
        this.changed = new Emitter();
        this.onDue = null;
        this._id = timers.add(10 * 1000, () => {
            this._check();
            return true;
        });
    }

    get tasks() {
        return this._store.data.tasks;
    }

    // start/end as minutes after midnight (older saves only have `due`)
    startOf(t) {
        return t.startMin ?? (t.due ? minutesOf(t.due) : null);
    }

    isDone(t) {
        return t.repeat === 'daily' ? t.doneDay === todayKey() : !!t.done;
    }

    // tasks that count today: daily ones, open ones, and one-off tasks finished today
    visible() {
        const today = todayKey();
        return this.tasks.filter(t => t.repeat === 'daily' || !t.done || !t.doneDay || t.doneDay === today);
    }

    stats() {
        const v = this.visible();
        return {done: v.filter(t => this.isDone(t)).length, total: v.length};
    }

    add(title, {startMin = null, endMin = null, repeat = 'daily'} = {}) {
        const id = this._store.nextId();
        this.tasks.push({
            id, title, startMin, endMin, repeat, done: false, alerted: false,
            due: startMin === null ? null : nextOccurrence(startMin),
            color: TASK_COLORS[id % TASK_COLORS.length],
        });
        this._store.save();
        this.changed.emit();
    }

    update(id, title, {startMin = null, endMin = null, repeat = 'daily'} = {}) {
        const t = this.tasks.find(x => x.id === id);
        if (!t) return;
        Object.assign(t, {title, startMin, endMin, repeat, alerted: false});
        t.due = startMin === null ? null : nextOccurrence(startMin);
        if (repeat === 'daily') t.done = false;
        this._store.save();
        this.changed.emit();
    }

    toggle(id) {
        const t = this.tasks.find(x => x.id === id);
        if (!t) return;
        if (this.isDone(t)) {
            t.done = false;
            t.doneDay = null;
        } else {
            t.done = true;
            t.doneDay = todayKey();
        }
        const start = this.startOf(t);
        if (t.repeat === 'daily' && start !== null) {
            t.due = nextOccurrence(start);
            t.alerted = false;
        }
        this._store.save();
        this.changed.emit();
    }

    remove(id) {
        const i = this.tasks.findIndex(x => x.id === id);
        if (i >= 0) this.tasks.splice(i, 1);
        this._store.save();
        this.changed.emit();
    }

    snooze(id, minutes) {
        const t = this.tasks.find(x => x.id === id);
        if (!t) return;
        t.due = Date.now() + minutes * 60 * 1000;
        t.alerted = false;
        this._store.save();
        this.changed.emit();
    }

    _check() {
        const now = Date.now();
        const midnight = startOfToday();
        let rolled = false;
        for (const t of this.tasks) {
            // a daily task left over from an earlier day comes back fresh
            const start = this.startOf(t);
            if (t.repeat === 'daily' && start !== null && t.due && t.due < midnight) {
                t.due = nextOccurrence(start);
                t.alerted = false;
                rolled = true;
            }
        }
        const t = this.tasks.find(x => !this.isDone(x) && x.due && !x.alerted && x.due <= now);
        if (!t && !rolled) return;
        if (t) t.alerted = true;
        this._store.save();
        this.changed.emit();
        if (t) {
            playChime();
            this.onDue?.(t);
        }
    }

    destroy() {
        // timer is owned (and cleared) by the shared Timers helper
    }
}

// ---------------------------------------------------------------- calendar reminders
export class CalendarService {
    constructor(store, timers) {
        this._store = store;
        this.changed = new Emitter();
        this.onDue = null;
        this._id = timers.add(10 * 1000, () => {
            this._check();
            return true;
        });
    }

    get items() {
        return (this._store.data.reminders ??= []);
    }

    on(key) {
        return this.items.filter(r => r.date === key).sort((a, b) => (a.startMin ?? -1) - (b.startMin ?? -1));
    }

    // reminders without a time ring at 09:00
    add(date, title, startMin = null) {
        const due = dateKeyToMs(date, startMin ?? 540);
        this.items.push({id: this._store.nextId(), title, date, startMin, due, alerted: due <= Date.now()});
        this._store.save();
        this.changed.emit();
    }

    remove(id) {
        const i = this.items.findIndex(r => r.id === id);
        if (i >= 0) this.items.splice(i, 1);
        this._store.save();
        this.changed.emit();
    }

    snooze(id, minutes) {
        const r = this.items.find(x => x.id === id);
        if (!r) return;
        r.due = Date.now() + minutes * 60 * 1000;
        r.alerted = false;
        this._store.save();
        this.changed.emit();
    }

    _check() {
        const now = Date.now();
        const r = this.items.find(x => !x.alerted && x.due <= now);
        if (!r) return;
        r.alerted = true;
        this._store.save();
        this.changed.emit();
        playChime();
        this.onDue?.(r);
    }
}

// ---------------------------------------------------------------- system peeks
const UPOWER_DISPLAY = '/org/freedesktop/UPower/devices/DisplayDevice';

async function sysGet(name, path, iface, prop) {
    const res = await Gio.DBus.system.call(
        name, path, 'org.freedesktop.DBus.Properties', 'Get',
        new GLib.Variant('(ss)', [iface, prop]), new GLib.VariantType('(v)'),
        Gio.DBusCallFlags.NONE, 1000, null);
    return res.deepUnpack()[0].deepUnpack();
}

export const BATTERY_STATE = {
    1: 'Charging', 2: 'On battery', 3: 'Empty', 4: 'Fully charged',
    5: 'Waiting to charge', 6: 'Waiting to discharge',
};

export class SystemWatcher {
    constructor() {
        this.battery = {present: false, percent: 0, state: 0, timeToFull: 0, timeToEmpty: 0};
        this.changed = new Emitter();
        this.onPeek = null; // (icon, text, ms)
        this._subs = [];
        this._netState = 0;
        this._lowWarned = false;
        this._capsSig = 0;
        this._keymap = null;
    }

    async start() {
        const sys = Gio.DBus.system;
        const sub = (sender, iface, member, path, arg0, cb) =>
            this._subs.push(sys.signal_subscribe(sender, iface, member, path, arg0, Gio.DBusSignalFlags.NONE, cb));

        // battery
        await this._readBattery(true);
        sub('org.freedesktop.UPower', 'org.freedesktop.DBus.Properties', 'PropertiesChanged',
            UPOWER_DISPLAY, null, () => this._readBattery(false));

        // network
        try {
            this._netState = await sysGet('org.freedesktop.NetworkManager',
                '/org/freedesktop/NetworkManager', 'org.freedesktop.NetworkManager', 'State');
        } catch (_e) { /* no NetworkManager */ }
        sub('org.freedesktop.NetworkManager', 'org.freedesktop.NetworkManager', 'StateChanged',
            '/org/freedesktop/NetworkManager', null, (_c, _s, _p, _i, _sig, params) => {
                const [state] = params.deepUnpack();
                const was = this._netState;
                this._netState = state;
                if (was >= 70 && state < 70) this.onPeek?.('network-wireless-offline-symbolic', 'Connection lost', 3000);
                else if (was < 70 && state >= 70 && was !== 0) this.onPeek?.('network-wireless-signal-excellent-symbolic', 'Connected', 2200);
            });

        // bluetooth headphones
        sub('org.bluez', 'org.freedesktop.DBus.Properties', 'PropertiesChanged', null,
            'org.bluez.Device1', (_c, _s, path, _i, _sig, params) => {
                const [, changed] = params.deepUnpack();
                if ('Connected' in changed) this._btChanged(path, changed.Connected.deepUnpack());
                this.changed.emit();
            });

        // caps lock
        try {
            this._keymap = getSeat().get_keymap();
            this._caps = this._keymap.get_caps_lock_state();
            this._capsSig = this._keymap.connect('state-changed', () => {
                const on = this._keymap.get_caps_lock_state();
                if (on === this._caps) return;
                this._caps = on;
                this.onPeek?.('input-keyboard-symbolic', on ? 'Caps Lock On' : 'Caps Lock Off', 1600);
            });
        } catch (_e) { /* keymap unavailable */ }
    }

    async _readBattery(initial) {
        try {
            const [present, pct, state, tf, te] = await Promise.all([
                sysGet('org.freedesktop.UPower', UPOWER_DISPLAY, 'org.freedesktop.UPower.Device', 'IsPresent'),
                sysGet('org.freedesktop.UPower', UPOWER_DISPLAY, 'org.freedesktop.UPower.Device', 'Percentage'),
                sysGet('org.freedesktop.UPower', UPOWER_DISPLAY, 'org.freedesktop.UPower.Device', 'State'),
                sysGet('org.freedesktop.UPower', UPOWER_DISPLAY, 'org.freedesktop.UPower.Device', 'TimeToFull'),
                sysGet('org.freedesktop.UPower', UPOWER_DISPLAY, 'org.freedesktop.UPower.Device', 'TimeToEmpty'),
            ]);
            const prev = this.battery;
            this.battery = {present, percent: Math.round(pct), state, timeToFull: Number(tf), timeToEmpty: Number(te)};
            if (!initial && present && prev.state !== state) {
                if (state === 1) this.onPeek?.('battery-full-charging-symbolic', `Charging · ${this.battery.percent}%`, 2800);
                else if (state === 2 && prev.state === 1) this.onPeek?.('battery-good-symbolic', `On battery · ${this.battery.percent}%`, 2500);
                else if (state === 4) this.onPeek?.('battery-full-charged-symbolic', 'Fully charged', 2800);
            }
            if (present && state === 2 && this.battery.percent <= 15 && !this._lowWarned) {
                this._lowWarned = true;
                this.onPeek?.('battery-caution-symbolic', `Battery low · ${this.battery.percent}%`, 4000);
            }
            if (state !== 2 || this.battery.percent > 20) this._lowWarned = false;
            this.changed.emit();
        } catch (_e) {
            this.battery = {present: false, percent: 0, state: 0, timeToFull: 0, timeToEmpty: 0};
        }
    }

    async _btChanged(path, connected) {
        try {
            const icon = await sysGet('org.bluez', path, 'org.bluez.Device1', 'Icon');
            if (!/headphone|headset|audio/.test(icon)) return;
            const name = await sysGet('org.bluez', path, 'org.bluez.Device1', 'Alias');
            this.onPeek?.('audio-headphones-symbolic', connected ? `${name} connected` : `${name} disconnected`, 3000);
        } catch (_e) { /* not an audio device */ }
    }

    // -> [{name, icon, battery|null}] for connected Bluetooth audio devices
    async connectedHeadphones() {
        try {
            const res = await Gio.DBus.system.call(
                'org.bluez', '/', 'org.freedesktop.DBus.ObjectManager', 'GetManagedObjects',
                null, new GLib.VariantType('(a{oa{sa{sv}}})'), Gio.DBusCallFlags.NONE, 1500, null);
            const [objs] = res.deepUnpack();
            const out = [];
            for (const path in objs) {
                const dev = objs[path]['org.bluez.Device1'];
                if (!dev) continue;
                const d = {};
                for (const k in dev) d[k] = dev[k].deepUnpack();
                if (!d.Connected || !/headphone|headset|audio/.test(d.Icon ?? '')) continue;
                const bat = objs[path]['org.bluez.Battery1'];
                out.push({name: d.Alias ?? d.Name ?? 'Headphones', battery: bat ? bat['Percentage'].deepUnpack() : null});
            }
            return out;
        } catch (_e) {
            return [];
        }
    }

    destroy() {
        for (const id of this._subs) Gio.DBus.system.signal_unsubscribe(id);
        this._subs = [];
        if (this._keymap && this._capsSig) this._keymap.disconnect(this._capsSig);
    }
}

// ---------------------------------------------------------------- idle mascot
export class IdleWatcher {
    constructor(onState) {
        this._cb = onState;
        this._mon = global.backend.get_core_idle_monitor();
        this._ids = [
            this._mon.add_idle_watch(45 * 1000, () => this._idle('eyes')),
            this._mon.add_idle_watch(4 * 60 * 1000, () => this._idle('doze')),
        ];
        this._active = 0;
    }

    _idle(state) {
        this._cb(state);
        if (this._active) return;
        this._active = this._mon.add_user_active_watch(() => {
            this._active = 0;
            this._cb('awake');
        });
    }

    destroy() {
        for (const id of this._ids) this._mon.remove_watch(id);
        if (this._active) this._mon.remove_watch(this._active);
        this._ids = [];
        this._active = 0;
    }
}
