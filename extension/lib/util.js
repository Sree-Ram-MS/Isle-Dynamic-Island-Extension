import GLib from 'gi://GLib';

// Tracks GLib timeouts so everything is removed when the extension is disabled.
export class Timers {
    constructor() {
        this._ids = new Set();
    }

    // fn returns true to keep repeating
    add(ms, fn) {
        const id = GLib.timeout_add(GLib.PRIORITY_DEFAULT, ms, () => {
            let again = false;
            try {
                again = !!fn();
            } catch (e) {
                console.error(`[nusa-notch] timer error: ${e.message}`);
            }
            if (!again) this._ids.delete(id);
            return again ? GLib.SOURCE_CONTINUE : GLib.SOURCE_REMOVE;
        });
        this._ids.add(id);
        return id;
    }

    clear(id) {
        if (id && this._ids.has(id)) {
            GLib.source_remove(id);
            this._ids.delete(id);
        }
    }

    destroy() {
        for (const id of this._ids) GLib.source_remove(id);
        this._ids.clear();
    }
}

// Minimal observer helper
export class Emitter {
    constructor() {
        this._cbs = new Set();
    }

    connect(cb) {
        this._cbs.add(cb);
        return () => this._cbs.delete(cb);
    }

    emit(...args) {
        for (const cb of [...this._cbs]) {
            try {
                cb(...args);
            } catch (e) {
                console.error(`[nusa-notch] listener error: ${e.message}`);
            }
        }
    }
}

export function fmtTime(ms) {
    const total = Math.max(0, Math.floor(ms / 1000));
    const m = Math.floor(total / 60);
    const s = total % 60;
    return `${m}:${String(s).padStart(2, '0')}`;
}

export function todayKey() {
    return GLib.DateTime.new_now_local().format('%Y-%m-%d');
}

export function dayKeyOffset(daysAgo) {
    return GLib.DateTime.new_now_local().add_days(-daysAgo).format('%Y-%m-%d');
}

export function weekdayLetter(daysAgo) {
    return GLib.DateTime.new_now_local().add_days(-daysAgo).format('%a').slice(0, 2);
}

// "HH:MM" -> next occurrence as epoch ms (today if still ahead, otherwise tomorrow)
export function parseClock(text) {
    const m = /^\s*(\d{1,2})[:.](\d{2})\s*$/.exec(text ?? '');
    if (!m) return null;
    const h = parseInt(m[1], 10);
    const min = parseInt(m[2], 10);
    if (h > 23 || min > 59) return null;
    const now = GLib.DateTime.new_now_local();
    let due = GLib.DateTime.new_local(now.get_year(), now.get_month(), now.get_day_of_month(), h, min, 0);
    if (due.to_unix() * 1000 <= Date.now()) due = due.add_days(1);
    return due.to_unix() * 1000;
}

export function clockLabel(ms, use24h) {
    const dt = GLib.DateTime.new_from_unix_local(Math.floor(ms / 1000));
    return dt.format(use24h ? '%H:%M' : '%-I:%M %p');
}
