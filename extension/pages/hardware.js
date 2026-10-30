import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import St from 'gi://St';

import {HardwareWatcher, fmtRate} from '../lib/hardware.js';
import {hbox, makeIcon, makeLabel, thinBar, vbox} from '../lib/ui.js';

const CYAN = '#3cc6f4';
const AMBER = '#f0b429';
const RED = '#ef5350';

export class HardwarePage {
    constructor(ctx) {
        this.id = 'hardware';
        this.icon = 'computer-symbolic';
        this.name = 'Hardware';
        this.title = 'Hardware Monitor';
        this._ctx = ctx;
        this._hw = new HardwareWatcher();
        this._timer = 0;
        this._build();
        this._hw.poll(); // baseline for the CPU and network deltas
    }

    // one tile: coloured icon, small label, value and an optional progress bar
    _tile(iconName, color, label, withBar) {
        const {px} = this._ctx;
        const icon = makeIcon('image-missing-symbolic', px(18));
        icon.gicon = new Gio.FileIcon({file: Gio.File.new_for_path(`${this._ctx.path}/icons/${iconName}-symbolic.svg`)});
        icon.set_style(`color: ${color};`);
        icon.y_align = Clutter.ActorAlign.CENTER;

        const lab = makeLabel(label, 'nusa-hlabel', {ellipsize: false});
        const val = makeLabel('--', 'nusa-hvalue', {ellipsize: false});
        const col = vbox(px(1), {x_expand: true, y_align: Clutter.ActorAlign.CENTER});
        col.add_child(lab);
        col.add_child(val);
        const t = {};
        if (withBar) {
            t.bar = thinBar(px(4), color);
            col.add_child(t.bar.actor);
        }
        const tile = hbox(px(9), {style_class: 'nusa-hcard', x_expand: true, y_expand: true, y_align: Clutter.ActorAlign.FILL});
        tile.add_child(icon);
        tile.add_child(col);
        t.tile = tile;
        t.val = val;
        return t;
    }

    _build() {
        const {px} = this._ctx;
        this._t = {
            cpu: this._tile('cpu', CYAN, 'CPU', true),
            ram: this._tile('ram', AMBER, 'RAM', true),
            gpu: this._tile('gpu', RED, 'GPU', true),
            up: this._tile('net-up', CYAN, 'NET UP', false),
            down: this._tile('net-down', CYAN, 'NET DOWN', false),
            disk: this._tile('disk', CYAN, 'DISK', true),
        };
        const colL = vbox(px(5), {x_expand: true, y_expand: true});
        const colR = vbox(px(5), {x_expand: true, y_expand: true});
        for (const k of ['cpu', 'ram', 'gpu']) colL.add_child(this._t[k].tile);
        for (const k of ['up', 'down', 'disk']) colR.add_child(this._t[k].tile);
        const root = hbox(px(8), {x_expand: true, y_expand: true});
        root.add_child(colL);
        root.add_child(colR);
        this.actor = new St.Bin({x_expand: true, y_expand: true, child: root});
    }

    _bar(t, pct) {
        t.bar?.set(pct / 100);
    }

    refresh() {
        const hw = this._hw;
        const gb = b => (b / 1073741824).toFixed(1);
        this._t.cpu.val.text = `${hw.cpu}%`;
        this._t.ram.val.text = `${gb(hw.ramUsed)} / ${gb(hw.ramTotal)} GB`;
        this._t.gpu.val.text = hw.gpu === null ? 'N/A' : `${hw.gpu}%`;
        this._t.up.val.text = fmtRate(hw.up);
        this._t.down.val.text = fmtRate(hw.down);
        this._t.disk.val.text = hw.diskTotal ? `${hw.disk}% · ${gb(hw.diskUsed)} / ${gb(hw.diskTotal)} GB` : `${hw.disk}%`;
        this._bar(this._t.cpu, hw.cpu);
        this._bar(this._t.ram, hw.ramTotal ? (100 * hw.ramUsed) / hw.ramTotal : 0);
        this._bar(this._t.gpu, hw.gpu ?? 0);
        this._bar(this._t.disk, hw.disk);
    }

    onShow() {
        this._hw.poll();
        this.refresh();
        this._ctx.timers.clear(this._timer);
        this._timer = this._ctx.timers.add(1500, () => {
            this._hw.poll();
            this.refresh();
            return true;
        });
    }

    onHide() {
        this._ctx.timers.clear(this._timer);
        this._timer = 0;
    }

    destroy() {
        this.onHide();
        this.actor.destroy();
    }
}
