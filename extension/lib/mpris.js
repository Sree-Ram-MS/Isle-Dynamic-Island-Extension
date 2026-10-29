import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Soup from 'gi://Soup?version=3.0';

import {Emitter} from './util.js';

Gio._promisify(Gio.DBusConnection.prototype, 'call', 'call_finish');
Gio._promisify(Soup.Session.prototype, 'send_and_read_async', 'send_and_read_finish');

const PREFIX = 'org.mpris.MediaPlayer2.';
const OBJ = '/org/mpris/MediaPlayer2';
const PLAYER_IFACE = 'org.mpris.MediaPlayer2.Player';

const PLAYER_XML = `<node><interface name="${PLAYER_IFACE}">
  <method name="Next"/><method name="Previous"/><method name="PlayPause"/>
  <method name="Play"/><method name="Pause"/>
  <property name="PlaybackStatus" type="s" access="read"/>
  <property name="Metadata" type="a{sv}" access="read"/>
  <property name="CanSeek" type="b" access="read"/>
</interface></node>`;
const PlayerProxy = Gio.DBusProxy.makeProxyWrapper(PLAYER_XML);

function unpackMeta(raw) {
    const meta = {};
    try {
        for (const k in raw) meta[k] = raw[k].deepUnpack();
    } catch (_e) {
        // ignore malformed metadata
    }
    return meta;
}

// Watches every MPRIS player on the session bus (Spotify, Firefox, VLC, Rhythmbox, ...)
export class MprisWatcher {
    constructor() {
        this.changed = new Emitter();
        this._players = new Map();
        this._lastActive = null;
        this._pos = {ms: 0, at: Date.now()};
        this._nameSub = 0;
        this._destroyed = false;
    }

    async start() {
        const bus = Gio.DBus.session;
        this._nameSub = bus.signal_subscribe(
            'org.freedesktop.DBus', 'org.freedesktop.DBus', 'NameOwnerChanged',
            '/org/freedesktop/DBus', null, Gio.DBusSignalFlags.NONE,
            (_c, _s, _p, _i, _sig, params) => {
                const [name, , newOwner] = params.deepUnpack();
                if (!name.startsWith(PREFIX)) return;
                if (newOwner) this._add(name);
                else this._remove(name);
            });
        try {
            const res = await bus.call(
                'org.freedesktop.DBus', '/org/freedesktop/DBus', 'org.freedesktop.DBus', 'ListNames',
                null, new GLib.VariantType('(as)'), Gio.DBusCallFlags.NONE, -1, null);
            const [names] = res.deepUnpack();
            for (const n of names) if (n.startsWith(PREFIX)) this._add(n);
        } catch (e) {
            console.error(`[nusa-notch] MPRIS ListNames failed: ${e.message}`);
        }
    }

    _add(name) {
        if (this._players.has(name)) return;
        const entry = {name, proxy: null, status: 'Stopped', meta: {}, sig: 0};
        this._players.set(name, entry);
        new PlayerProxy(Gio.DBus.session, name, OBJ, (proxy, err) => {
            if (err || this._destroyed || !this._players.has(name)) {
                this._players.delete(name);
                return;
            }
            entry.proxy = proxy;
            entry.sig = proxy.connect('g-properties-changed', () => this._sync(entry));
            this._sync(entry);
        }, null, Gio.DBusProxyFlags.DO_NOT_AUTO_START);
    }

    _remove(name) {
        const entry = this._players.get(name);
        if (!entry) return;
        if (entry.proxy && entry.sig) entry.proxy.disconnect(entry.sig);
        this._players.delete(name);
        if (this._lastActive === name) this._lastActive = null;
        this.changed.emit();
    }

    _sync(entry) {
        const proxy = entry.proxy;
        if (!proxy) return;
        const before = this.trackKey();
        entry.status = proxy.PlaybackStatus ?? 'Stopped';
        entry.meta = unpackMeta(proxy.Metadata);
        if (entry.status === 'Playing') this._lastActive = entry.name;
        if (this.active === entry && before !== this.trackKey()) this._pos = {ms: 0, at: Date.now()};
        if (this.active === entry) this.refreshPosition();
        this.changed.emit();
    }

    get active() {
        const list = [...this._players.values()].filter(p => p.proxy);
        const playing = list.filter(p => p.status === 'Playing');
        if (playing.length) return playing.find(p => p.name === this._lastActive) ?? playing[0];
        const last = list.find(p => p.name === this._lastActive && p.meta['xesam:title']);
        return last ?? list.find(p => p.status === 'Paused' && p.meta['xesam:title']) ?? null;
    }

    get hasPlayer() {
        return !!this.active;
    }

    get playing() {
        return this.active?.status === 'Playing';
    }

    get track() {
        const a = this.active;
        if (!a) return null;
        const m = a.meta;
        const artist = Array.isArray(m['xesam:artist']) ? m['xesam:artist'].join(', ') : (m['xesam:artist'] ?? '');
        return {
            title: m['xesam:title'] ?? '',
            artist,
            album: m['xesam:album'] ?? '',
            artUrl: m['mpris:artUrl'] ?? '',
            lengthMs: Math.round(Number(m['mpris:length'] ?? 0) / 1000),
            trackId: m['mpris:trackid'] ?? '/org/mpris/MediaPlayer2/TrackList/NoTrack',
        };
    }

    trackKey() {
        const t = this.track;
        return t ? `${this.active.name}|${t.title}|${t.artist}|${t.lengthMs}` : '';
    }

    // ---- position -------------------------------------------------------
    get positionMs() {
        const p = this._pos;
        return this.playing ? p.ms + (Date.now() - p.at) : p.ms;
    }

    async refreshPosition() {
        const a = this.active;
        if (!a) return;
        try {
            const res = await Gio.DBus.session.call(
                a.name, OBJ, 'org.freedesktop.DBus.Properties', 'Get',
                new GLib.Variant('(ss)', [PLAYER_IFACE, 'Position']),
                new GLib.VariantType('(v)'), Gio.DBusCallFlags.NONE, 500, null);
            const us = Number(res.deepUnpack()[0].deepUnpack());
            this._pos = {ms: us / 1000, at: Date.now()};
        } catch (_e) {
            // some players do not expose Position
        }
    }

    // ---- control --------------------------------------------------------
    _method(name) {
        this.active?.proxy?.[`${name}Remote`]?.();
    }

    playPause() { this._method('PlayPause'); }
    next() { this._method('Next'); }
    previous() { this._method('Previous'); }

    async seekTo(ms) {
        const a = this.active;
        const t = this.track;
        if (!a || !t) return;
        this._pos = {ms, at: Date.now()};
        try {
            await Gio.DBus.session.call(
                a.name, OBJ, PLAYER_IFACE, 'SetPosition',
                new GLib.Variant('(ox)', [t.trackId, Math.round(ms * 1000)]),
                null, Gio.DBusCallFlags.NONE, 500, null);
        } catch (_e) {
            // fall back to relative seek
            try {
                const delta = Math.round((ms - this.positionMs) * 1000);
                await Gio.DBus.session.call(
                    a.name, OBJ, PLAYER_IFACE, 'Seek', new GLib.Variant('(x)', [delta]),
                    null, Gio.DBusCallFlags.NONE, 500, null);
            } catch (_e2) { /* player does not support seeking */ }
        }
    }

    async getVolume() {
        const a = this.active;
        if (!a) return null;
        try {
            const res = await Gio.DBus.session.call(
                a.name, OBJ, 'org.freedesktop.DBus.Properties', 'Get',
                new GLib.Variant('(ss)', [PLAYER_IFACE, 'Volume']),
                new GLib.VariantType('(v)'), Gio.DBusCallFlags.NONE, 500, null);
            return Number(res.deepUnpack()[0].deepUnpack());
        } catch (_e) {
            return null;
        }
    }

    async setVolume(v) {
        const a = this.active;
        if (!a) return;
        try {
            await Gio.DBus.session.call(
                a.name, OBJ, 'org.freedesktop.DBus.Properties', 'Set',
                new GLib.Variant('(ssv)', [PLAYER_IFACE, 'Volume', new GLib.Variant('d', Math.max(0, Math.min(1, v)))]),
                null, Gio.DBusCallFlags.NONE, 500, null);
        } catch (_e) { /* volume not settable on this player */ }
    }

    destroy() {
        this._destroyed = true;
        if (this._nameSub) Gio.DBus.session.signal_unsubscribe(this._nameSub);
        for (const e of this._players.values())
            if (e.proxy && e.sig) e.proxy.disconnect(e.sig);
        this._players.clear();
    }
}

// ---- album art cache ------------------------------------------------------
const artSession = {s: null};

// Resolves to a local file path (or null). Remote art is cached in ~/.cache/nusa-notch/art.
export async function resolveArt(url) {
    if (!url) return null;
    if (url.startsWith('file://')) return Gio.File.new_for_uri(url).get_path();
    if (!/^https?:\/\//.test(url)) return null;
    const dir = GLib.build_filenamev([GLib.get_user_cache_dir(), 'nusa-notch', 'art']);
    GLib.mkdir_with_parents(dir, 0o755);
    const path = GLib.build_filenamev([dir, GLib.compute_checksum_for_string(GLib.ChecksumType.SHA1, url, -1)]);
    if (GLib.file_test(path, GLib.FileTest.EXISTS)) return path;
    try {
        artSession.s ??= new Soup.Session({timeout: 10});
        const msg = Soup.Message.new('GET', url);
        const bytes = await artSession.s.send_and_read_async(msg, GLib.PRIORITY_DEFAULT, null);
        if (msg.get_status() !== 200) return null;
        GLib.file_set_contents(path, bytes.get_data());
        return path;
    } catch (_e) {
        return null;
    }
}

export function destroyArtSession() {
    artSession.s?.abort();
    artSession.s = null;
}
