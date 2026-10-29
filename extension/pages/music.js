import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import St from 'gi://St';

import {Slider} from 'resource:///org/gnome/shell/ui/slider.js';

import {resolveArt} from '../lib/mpris.js';
import {SystemVolume} from '../lib/volume.js';
import {hbox, iconButton, makeIcon, makeLabel, spacer, vbox} from '../lib/ui.js';

const ART = 84;

// 00:00 style (zero-padded minutes)
function clock(ms) {
    const total = Math.max(0, Math.floor(ms / 1000));
    const m = Math.floor(total / 60);
    const s = total % 60;
    return `${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
}

export class MusicPage {
    constructor(ctx) {
        this.id = 'music';
        this.icon = 'audio-x-generic-symbolic';
        this.name = 'Music';
        this.title = '';
        this._ctx = ctx;
        this._tick = 0;
        this._ticks = 0;
        this._dragging = false;
        this._volDragging = false;
        this._volAcc = 0;
        this._sysvol = new SystemVolume();
        this._artKey = '';
        this._build();
    }

    _build() {
        const {px} = this._ctx;
        this.actor = new St.Bin({x_expand: true, y_expand: true});

        const art = px(ART);
        this._art = new St.Widget({style_class: 'nusa-art', width: art, height: art, y_align: Clutter.ActorAlign.CENTER, layout_manager: new Clutter.BinLayout()});
        this._artIcon = makeIcon('audio-x-generic-symbolic', px(32), 'nusa-dim');
        this._artIcon.x_align = Clutter.ActorAlign.CENTER;
        this._artIcon.x_expand = this._artIcon.y_expand = true;
        this._art.add_child(this._artIcon);

        this._title = makeLabel('Not Playing', 'nusa-title');
        this._artist = makeLabel('', 'nusa-sub');
        this._elapsed = makeLabel('00:00', 'nusa-dim', {ellipsize: false});
        this._total = makeLabel('-00:00', 'nusa-dim', {ellipsize: false});

        this._seek = new Slider(0);
        this._seek.add_style_class_name('nusa-slider');
        this._seek.x_expand = true;
        this._seek.reactive = false;
        this._seek.connect('drag-begin', () => { this._dragging = true; });
        this._seek.connect('drag-end', () => {
            this._dragging = false;
            const t = this._ctx.mpris.track;
            if (t?.lengthMs) this._ctx.mpris.seekTo(this._seek.value * t.lengthMs);
        });

        const seekRow = hbox(px(8), {x_expand: true});
        seekRow.add_child(this._elapsed);
        seekRow.add_child(this._seek);
        seekRow.add_child(this._total);

        // volume slider (hidden until the speaker button is clicked)
        this._vol = new Slider(1);
        this._vol.add_style_class_name('nusa-slider');
        this._vol.x_expand = true;
        this._vol.connect('drag-begin', () => { this._volDragging = true; });
        this._vol.connect('drag-end', () => { this._volDragging = false; });
        this._vol.connect('notify::value', () => {
            this._updateVolIcon();
            if (this._volDragging) this._sysvol.set(this._vol.value);
        });
        this._vol.connect('scroll-event', (_a, ev) => this._onVolScroll(ev));
        this._volRow = hbox(px(8), {x_expand: true, visible: false});
        this._volRow.add_child(this._vol);

        // bottom row: [chat]   [rewind play forward]   [speaker]
        const chatBtn = iconButton(this._ctx, 'chat-symbolic', () => this._ctx.notch.gotoPage('lyrics'), {size: 16});
        chatBtn.child.gicon = new Gio.FileIcon({file: Gio.File.new_for_path(`${this._ctx.path}/icons/chat-symbolic.svg`)});

        this._playBtn = iconButton(this._ctx, 'media-playback-start-symbolic', () => this._ctx.mpris.playPause(), {size: 22});
        const controls = hbox(px(6), {y_align: Clutter.ActorAlign.CENTER});
        controls.add_child(iconButton(this._ctx, 'media-seek-backward-symbolic', () => this._ctx.mpris.previous(), {size: 18}));
        controls.add_child(this._playBtn);
        controls.add_child(iconButton(this._ctx, 'media-seek-forward-symbolic', () => this._ctx.mpris.next(), {size: 18}));

        this._volBtn = iconButton(this._ctx, 'audio-volume-high-symbolic', () => {
            this._volRow.visible = !this._volRow.visible;
        }, {size: 16});
        this._volBtn.connect('scroll-event', (_a, ev) => this._onVolScroll(ev));

        const ctlRow = hbox(px(6), {x_expand: true});
        ctlRow.add_child(chatBtn);
        ctlRow.add_child(spacer());
        ctlRow.add_child(controls);
        ctlRow.add_child(spacer());
        ctlRow.add_child(this._volBtn);

        const col = vbox(px(2), {x_expand: true, y_align: Clutter.ActorAlign.CENTER});
        col.add_child(this._title);
        col.add_child(this._artist);
        col.add_child(seekRow);
        col.add_child(this._volRow);
        col.add_child(ctlRow);

        this._player = hbox(px(14), {x_expand: true, y_expand: true});
        this._player.add_child(this._art);
        this._player.add_child(col);
        this.actor.set_child(this._player);

        this.refresh();
    }

    // scroll over the speaker button / slider changes the volume
    _onVolScroll(ev) {
        const dir = ev.get_scroll_direction();
        let d = 0;
        if (dir === Clutter.ScrollDirection.UP) d = 1;
        else if (dir === Clutter.ScrollDirection.DOWN) d = -1;
        else if (dir === Clutter.ScrollDirection.SMOOTH) {
            this._volAcc += -ev.get_scroll_delta()[1];
            if (Math.abs(this._volAcc) < 0.5) return Clutter.EVENT_STOP;
            d = this._volAcc > 0 ? 1 : -1;
            this._volAcc = 0;
        }
        if (d) {
            const v = Math.max(0, Math.min(1, this._vol.value + d * 0.05));
            this._vol.value = v;
            this._sysvol.set(v);
        }
        return Clutter.EVENT_STOP;
    }

    _updateVolIcon() {
        const v = this._vol.value;
        const name = v <= 0.001 ? 'audio-volume-muted-symbolic'
            : v < 0.34 ? 'audio-volume-low-symbolic'
                : v < 0.67 ? 'audio-volume-medium-symbolic' : 'audio-volume-high-symbolic';
        if (this._volBtn.child.icon_name !== name) this._volBtn.child.icon_name = name;
    }

    refresh() {
        const m = this._ctx.mpris;
        const t = m.track;
        if (!t) {
            this._title.text = 'Not Playing';
            this._artist.text = '';
            this._elapsed.text = '00:00';
            this._total.text = '-00:00';
            this._seek.value = 0;
            this._seek.reactive = false;
            this._playBtn.child.icon_name = 'media-playback-start-symbolic';
            this._updateArt(null);
            this._syncVolume();
            return;
        }
        this._title.text = t.title || 'Unknown title';
        this._artist.text = t.artist || m.active?.name.replace('org.mpris.MediaPlayer2.', '') || '';
        this._playBtn.child.icon_name = m.playing ? 'media-playback-pause-symbolic' : 'media-playback-start-symbolic';
        this._seek.reactive = !!t.lengthMs;
        this._updateArt(t.artUrl);
        this._updatePosition();
        this._syncVolume();
    }

    _syncVolume() {
        this._sysvol.get().then(v => {
            if (v !== null && !this._volDragging && Math.abs(this._vol.value - v) > 0.005) this._vol.value = v;
        });
    }

    async _updateArt(url) {
        if (url === this._artKey) return;
        this._artKey = url;
        const path = await resolveArt(url);
        if (url !== this._artKey) return;
        const size = this._ctx.px(ART);
        if (path) {
            this._art.style = `background-image: url("file://${path}"); background-size: ${size}px ${size}px;`;
            this._artIcon.visible = false;
        } else {
            this._art.style = '';
            this._artIcon.visible = true;
        }
    }

    _updatePosition() {
        const m = this._ctx.mpris;
        const t = m.track;
        if (!t) return;
        const pos = Math.min(m.positionMs, t.lengthMs || m.positionMs);
        this._elapsed.text = clock(pos);
        this._total.text = t.lengthMs ? `-${clock(Math.max(0, t.lengthMs - pos))}` : '-00:00';
        if (!this._dragging && t.lengthMs) this._seek.value = Math.max(0, Math.min(1, pos / t.lengthMs));
    }

    onShow() {
        this._ctx.mpris.refreshPosition();
        this.refresh();
        this._ticks = 0;
        this._ctx.timers.clear(this._tick);
        this._tick = this._ctx.timers.add(500, () => {
            if (this._ctx.mpris.playing) {
                this._updatePosition();
                if (++this._ticks % 6 === 0) this._ctx.mpris.refreshPosition();
            }
            return true;
        });
    }

    onHide() {
        this._ctx.timers.clear(this._tick);
        this._tick = 0;
    }

    destroy() {
        this.onHide();
        this.actor.destroy();
    }
}
