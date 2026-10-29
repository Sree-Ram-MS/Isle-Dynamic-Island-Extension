import Clutter from 'gi://Clutter';
import St from 'gi://St';

import {Slider} from 'resource:///org/gnome/shell/ui/slider.js';

import {resolveArt} from '../lib/mpris.js';
import {fmtTime} from '../lib/util.js';
import {emptyState, hbox, iconButton, makeIcon, makeLabel, spacer, vbox} from '../lib/ui.js';

export class MusicPage {
    constructor(ctx) {
        this.id = 'music';
        this.icon = 'audio-x-generic-symbolic';
        this.name = 'Music';
        this._ctx = ctx;
        this._tick = 0;
        this._ticks = 0;
        this._dragging = false;
        this._volDragging = false;
        this._artKey = '';
        this._build();
    }

    _build() {
        const {px} = this._ctx;
        this.actor = new St.Bin({x_expand: true, y_expand: true});

        this._empty = emptyState(this._ctx, 'audio-x-generic-symbolic', 'Nothing playing',
            'Play something in Spotify, a browser, VLC…');

        const art = px(112);
        this._art = new St.Widget({style_class: 'nusa-art', width: art, height: art, y_align: Clutter.ActorAlign.CENTER, layout_manager: new Clutter.BinLayout()});
        this._artIcon = makeIcon('audio-x-generic-symbolic', px(40), 'nusa-dim');
        this._artIcon.x_align = Clutter.ActorAlign.CENTER;
        this._artIcon.x_expand = this._artIcon.y_expand = true;
        this._art.add_child(this._artIcon);

        this._title = makeLabel('', 'nusa-title');
        this._artist = makeLabel('', 'nusa-sub');
        this._elapsed = makeLabel('0:00', 'nusa-dim', {ellipsize: false});
        this._total = makeLabel('0:00', 'nusa-dim', {ellipsize: false});

        this._seek = new Slider(0);
        this._seek.add_style_class_name('nusa-slider');
        this._seek.x_expand = true;
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

        this._playBtn = iconButton(this._ctx, 'media-playback-start-symbolic', () => this._ctx.mpris.playPause(), {size: 22});
        const controls = hbox(px(4), {y_align: Clutter.ActorAlign.CENTER});
        controls.add_child(iconButton(this._ctx, 'media-skip-backward-symbolic', () => this._ctx.mpris.previous(), {size: 18}));
        controls.add_child(this._playBtn);
        controls.add_child(iconButton(this._ctx, 'media-skip-forward-symbolic', () => this._ctx.mpris.next(), {size: 18}));

        this._vol = new Slider(1);
        this._vol.add_style_class_name('nusa-slider');
        this._vol.set_width(px(80));
        this._vol.connect('drag-begin', () => { this._volDragging = true; });
        this._vol.connect('drag-end', () => { this._volDragging = false; });
        this._vol.connect('notify::value', () => {
            if (this._volDragging) this._ctx.mpris.setVolume(this._vol.value);
        });

        const lyricsBtn = iconButton(this._ctx, 'format-justify-left-symbolic', () => this._ctx.notch.gotoPage('lyrics'), {size: 16});

        const ctlRow = hbox(px(6), {x_expand: true});
        ctlRow.add_child(controls);
        ctlRow.add_child(spacer());
        ctlRow.add_child(makeIcon('audio-volume-medium-symbolic', px(14), 'nusa-dim'));
        ctlRow.add_child(this._vol);
        ctlRow.add_child(lyricsBtn);

        const col = vbox(px(2), {x_expand: true, y_align: Clutter.ActorAlign.CENTER});
        col.add_child(this._title);
        col.add_child(this._artist);
        col.add_child(seekRow);
        col.add_child(ctlRow);

        this._player = hbox(px(14), {x_expand: true, y_expand: true});
        this._player.add_child(this._art);
        this._player.add_child(col);

        this.refresh();
    }

    refresh() {
        const m = this._ctx.mpris;
        const t = m.track;
        if (!t) {
            this.actor.set_child(this._empty);
            return;
        }
        this.actor.set_child(this._player);
        this._title.text = t.title || 'Unknown title';
        this._artist.text = t.artist || m.active?.name.replace('org.mpris.MediaPlayer2.', '') || '';
        this._total.text = t.lengthMs ? fmtTime(t.lengthMs) : '--:--';
        this._playBtn.child.icon_name = m.playing ? 'media-playback-pause-symbolic' : 'media-playback-start-symbolic';
        this._seek.reactive = !!t.lengthMs;
        this._updateArt(t.artUrl);
        this._updatePosition();
        m.getVolume().then(v => {
            if (v !== null && !this._volDragging && this._vol.value !== v) this._vol.value = Math.max(0, Math.min(1, v));
        });
    }

    async _updateArt(url) {
        if (url === this._artKey) return;
        this._artKey = url;
        const path = await resolveArt(url);
        if (url !== this._artKey) return;
        const size = this._ctx.px(112);
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
        this._elapsed.text = fmtTime(pos);
        if (!this._dragging && t.lengthMs) this._seek.value = Math.max(0, Math.min(1, pos / t.lengthMs));
    }

    onShow() {
        this._ctx.mpris.refreshPosition();
        this.refresh();
        this._ticks = 0;
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
