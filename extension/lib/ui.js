import Clutter from 'gi://Clutter';
import Pango from 'gi://Pango';
import St from 'gi://St';

export function makeIcon(name, size, cls = '') {
    return new St.Icon({icon_name: name, icon_size: size, style_class: cls, y_align: Clutter.ActorAlign.CENTER});
}

export function makeLabel(text, cls = '', {ellipsize = true, xExpand = false} = {}) {
    const l = new St.Label({text, style_class: cls, y_align: Clutter.ActorAlign.CENTER, x_expand: xExpand});
    if (ellipsize) l.clutter_text.ellipsize = Pango.EllipsizeMode.END;
    return l;
}

export function iconButton(ctx, name, onClick, {size = 16, cls = 'nusa-ctl'} = {}) {
    const btn = new St.Button({
        style_class: cls, reactive: true, can_focus: true, track_hover: true,
        child: makeIcon(name, ctx.px(size)), y_align: Clutter.ActorAlign.CENTER,
    });
    btn.connect('clicked', () => {
        onClick();
        return Clutter.EVENT_STOP;
    });
    return btn;
}

export function pill(text, onClick, accent = false) {
    const btn = new St.Button({
        style_class: `nusa-pill${accent ? ' accent' : ''}`, label: text,
        reactive: true, can_focus: true, track_hover: true, y_align: Clutter.ActorAlign.CENTER,
    });
    btn.connect('clicked', () => {
        onClick();
        return Clutter.EVENT_STOP;
    });
    return btn;
}

// Fixed-width progress bar. set(0..1)
export function progressBar(width, extraClass = '') {
    const track = new St.Widget({style_class: 'nusa-track', width, y_align: Clutter.ActorAlign.CENTER, layout_manager: new Clutter.FixedLayout()});
    const fill = new St.Widget({style_class: `nusa-fill ${extraClass}`, width: 0, height: 5});
    track.add_child(fill);
    return {
        actor: track,
        set(frac) {
            fill.set_width(Math.round(width * Math.max(0, Math.min(1, frac))));
        },
    };
}

export function spacer() {
    return new St.Widget({x_expand: true, y_expand: true});
}

export function hbox(spacing = 0, props = {}) {
    return new St.BoxLayout({orientation: Clutter.Orientation.HORIZONTAL, style: `spacing: ${spacing}px;`, ...props});
}

export function vbox(spacing = 0, props = {}) {
    return new St.BoxLayout({orientation: Clutter.Orientation.VERTICAL, style: `spacing: ${spacing}px;`, ...props});
}

// Simple empty-state view
export function emptyState(ctx, iconName, title, sub) {
    const box = vbox(ctx.px(4), {x_expand: true, y_expand: true, x_align: Clutter.ActorAlign.CENTER, y_align: Clutter.ActorAlign.CENTER});
    box.add_child(makeIcon(iconName, ctx.px(34), 'nusa-dim'));
    box.add_child(makeLabel(title, 'nusa-title'));
    if (sub) box.add_child(makeLabel(sub, 'nusa-dim'));
    return box;
}

// Thin rounded progress bar that always fills from the left. set(0..1)
export function thinBar(height, color) {
    const track = new St.Widget({style_class: 'nusa-htrack', x_expand: true, height, layout_manager: new Clutter.FixedLayout()});
    const fill = new St.Widget({style_class: 'nusa-hfill'});
    fill.set_style(`background-color: ${color};`);
    fill.set_position(0, 0);
    fill.set_size(0, height);
    track.add_child(fill);
    let frac = 0;
    const apply = () => fill.set_size(Math.round(track.width * frac), height);
    track.connect('notify::allocation', apply);
    return {
        actor: track,
        set(f) {
            frac = Math.max(0, Math.min(1, f));
            apply();
        },
    };
}
