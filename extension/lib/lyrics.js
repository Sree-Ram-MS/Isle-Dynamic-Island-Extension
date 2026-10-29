import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Soup from 'gi://Soup?version=3.0';

Gio._promisify(Soup.Session.prototype, 'send_and_read_async', 'send_and_read_finish');

// Parse LRC text ("[01:23.45] line") into sorted [{t: ms, text}]
export function parseLrc(text) {
    const lines = [];
    for (const raw of String(text).split(/\r?\n/)) {
        const re = /\[(\d+):(\d+(?:[.:]\d+)?)\]/g;
        const stamps = [];
        let m;
        let end = 0;
        while ((m = re.exec(raw)) !== null) {
            stamps.push((parseInt(m[1], 10) * 60 + parseFloat(m[2].replace(':', '.'))) * 1000);
            end = re.lastIndex;
        }
        if (!stamps.length) continue;
        const body = raw.slice(end).trim();
        for (const t of stamps) lines.push({t, text: body});
    }
    lines.sort((a, b) => a.t - b.t);
    return lines;
}

// Index of the line being sung at `ms` (-1 before the first line)
export function lineIndexAt(lines, ms) {
    let lo = 0;
    let hi = lines.length - 1;
    let ans = -1;
    while (lo <= hi) {
        const mid = (lo + hi) >> 1;
        if (lines[mid].t <= ms) {
            ans = mid;
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
    }
    return ans;
}

// LRCLIB client. Only sends title, artist, album and length of the playing song.
export class LyricsClient {
    constructor() {
        this._session = new Soup.Session({timeout: 10, user_agent: 'nusa-notch-linux/1.0'});
        this._cache = new Map();
        this._cancel = null;
    }

    // -> array of lines, [] when none found, null when cancelled/failed
    async fetch(track) {
        const key = `${track.artist}|${track.title}|${track.lengthMs}`;
        if (this._cache.has(key)) return this._cache.get(key);

        this._cancel?.cancel();
        const cancellable = new Gio.Cancellable();
        this._cancel = cancellable;

        const q = (o) => Object.entries(o)
            .filter(([, v]) => v !== '' && v !== undefined && v !== 0)
            .map(([k, v]) => `${k}=${encodeURIComponent(v)}`).join('&');

        try {
            let data = await this._get(`https://lrclib.net/api/get?${q({
                artist_name: track.artist, track_name: track.title,
                album_name: track.album, duration: Math.round(track.lengthMs / 1000),
            })}`, cancellable);

            if (!data?.syncedLyrics) {
                const found = await this._get(`https://lrclib.net/api/search?${q({
                    artist_name: track.artist, track_name: track.title})}`, cancellable);
                if (Array.isArray(found))
                    data = found.find(r => r.syncedLyrics) ?? null;
            }
            const lines = data?.syncedLyrics ? parseLrc(data.syncedLyrics) : [];
            this._cache.set(key, lines);
            if (this._cache.size > 40) this._cache.delete(this._cache.keys().next().value);
            return lines;
        } catch (_e) {
            return null;
        }
    }

    async _get(url, cancellable) {
        const msg = Soup.Message.new('GET', url);
        const bytes = await this._session.send_and_read_async(msg, GLib.PRIORITY_DEFAULT, cancellable);
        if (msg.get_status() !== 200) return null;
        return JSON.parse(new TextDecoder().decode(bytes.get_data()));
    }

    destroy() {
        this._cancel?.cancel();
        this._session.abort();
    }
}
