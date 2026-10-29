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

// ---- minutes-of-day helpers (to-do times, water hours)
// "14:30", "2:30 pm", "2pm" -> minutes after midnight, or null
export function parseMinutes(text) {
    const m = /^\s*(\d{1,2})(?:[:.](\d{2}))?\s*([ap])?\.?m?\.?\s*$/i.exec(text ?? '');
    if (!m) return null;
    let h = parseInt(m[1], 10);
    const min = m[2] ? parseInt(m[2], 10) : 0;
    if (min > 59) return null;
    if (m[3]) {
        if (h < 1 || h > 12) return null;
        h = (h % 12) + (m[3].toLowerCase() === 'p' ? 12 : 0);
    } else if (!m[2]) {
        return null; // a bare number is ambiguous
    }
    if (h > 23) return null;
    return h * 60 + min;
}

// "09:00-10:30" | "9am - 10am" | "14:30" -> {start, end|null}, or null when invalid
export function parseRange(text) {
    const parts = (text ?? '').split(/\s*(?:-|–|—|\bto\b)\s*/i).filter(s => s.trim() !== '');
    if (parts.length < 1 || parts.length > 2) return null;
    const start = parseMinutes(parts[0]);
    if (start === null) return null;
    let end = null;
    if (parts.length === 2) {
        end = parseMinutes(parts[1]);
        if (end === null) return null;
    }
    return {start, end};
}

function ampm(min) {
    const h = Math.floor(min / 60) % 24;
    const m = min % 60;
    return [`${h % 12 || 12}:${String(m).padStart(2, '0')}`, h < 12 ? 'AM' : 'PM'];
}

// 9:00 AM  |  9:00 – 10:30 AM  |  11:00 AM – 1:00 PM
export function fmtRange(start, end) {
    const [s, sp] = ampm(start);
    if (end === null || end === undefined) return `${s} ${sp}`;
    const [e, ep] = ampm(end);
    return sp === ep ? `${s} – ${e} ${ep}` : `${s} ${sp} – ${e} ${ep}`;
}

// 540 -> "09:00" (24 h, used by the water hours picker)
export function fmtHM(min) {
    return `${String(Math.floor(min / 60)).padStart(2, '0')}:${String(min % 60).padStart(2, '0')}`;
}

export function minutesNow() {
    const n = GLib.DateTime.new_now_local();
    return n.get_hour() * 60 + n.get_minute();
}

export function minutesOf(ms) {
    const d = GLib.DateTime.new_from_unix_local(Math.floor(ms / 1000));
    return d.get_hour() * 60 + d.get_minute();
}

// next time the clock shows `min` minutes after midnight, as epoch ms
export function nextOccurrence(min) {
    const now = GLib.DateTime.new_now_local();
    let due = GLib.DateTime.new_local(now.get_year(), now.get_month(), now.get_day_of_month(),
        Math.floor(min / 60), min % 60, 0);
    if (due.to_unix() * 1000 <= Date.now()) due = due.add_days(1);
    return due.to_unix() * 1000;
}

export function startOfToday() {
    const n = GLib.DateTime.new_now_local();
    return GLib.DateTime.new_local(n.get_year(), n.get_month(), n.get_day_of_month(), 0, 0, 0).to_unix() * 1000;
}

export function pad2(n) {
    return String(n).padStart(2, '0');
}

export function dateKey(y, m, d) {
    return `${y}-${pad2(m)}-${pad2(d)}`;
}

// 'YYYY-MM-DD' + minutes after midnight -> epoch ms
export function dateKeyToMs(key, min = 0) {
    const [y, m, d] = key.split('-').map(Number);
    return GLib.DateTime.new_local(y, m, d, Math.floor(min / 60), min % 60, 0).to_unix() * 1000;
}
