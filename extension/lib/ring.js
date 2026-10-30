import Clutter from 'gi://Clutter';
import St from 'gi://St';

export const GREEN = [0.19, 0.82, 0.35];
export const AMBER = [1.0, 0.62, 0.04];
export const BLUE = [0.20, 0.55, 1.0];
export const RED = [1.0, 0.27, 0.23];
export const FOCUS_RGB = [0.91, 0.38, 0.42];
export const BREAK_RGB = [0.31, 0.72, 0.63];

// Small round indicator.
//  battery: ring whose green arc is the battery percentage
//  focus:   circle filled like a glass of water; the level is the time left (red = focus, teal = break)
export function makeRing(px, size) {
    const area = new St.DrawingArea({width: px(size), height: px(size), y_align: Clutter.ActorAlign.CENTER});
    const st = {mode: 'battery', frac: 1, rgb: GREEN, bolt: false};

    area.connect('repaint', a => {
        const [w, h] = a.get_surface_size();
        const lw = Math.max(2, Math.round(w * 0.16));
        const r = (Math.min(w, h) - lw) / 2;
        const cx = w / 2;
        const cy = h / 2;
        const [R, G, B] = st.rgb;
        const cr = a.get_context();
        cr.setLineWidth(lw);
        if (st.mode === 'battery') {
            cr.setSourceRGBA(R, G, B, 0.25);
            cr.arc(cx, cy, r, 0, 2 * Math.PI);
            cr.stroke();
            cr.setSourceRGBA(R, G, B, 1);
            cr.setLineCap(1);
            const f = Math.max(0.03, Math.min(1, st.frac));
            if (f >= 0.999) cr.arc(cx, cy, r, 0, 2 * Math.PI);
            else cr.arc(cx, cy, r, -Math.PI / 2, -Math.PI / 2 + 2 * Math.PI * f);
            cr.stroke();
            if (st.bolt) {
                // Draw the bolt in the same Cairo pass, using coordinates
                // centred exactly on the circle. (A text glyph looked off-
                // centre because fonts carry their own side bearings.)
                const pts = [
                    [-0.14, -0.64], [0.34, -0.64], [0.08, -0.14], [0.42, -0.14],
                    [-0.28, 0.64], [-0.10, 0.08], [-0.42, 0.08],
                ];
                cr.setSourceRGBA(R, G, B, 1);
                pts.forEach(([x, y], i) => {
                    if (i === 0) cr.moveTo(cx + x * r, cy + y * r);
                    else cr.lineTo(cx + x * r, cy + y * r);
                });
                cr.closePath();
                cr.fill();
            }
        } else {
            // outline
            cr.setSourceRGBA(R, G, B, 1);
            cr.arc(cx, cy, r, 0, 2 * Math.PI);
            cr.stroke();
            // water inside, clipped to the circle
            const ri = r - lw / 2 - Math.max(1, w * 0.06);
            cr.save();
            cr.arc(cx, cy, ri, 0, 2 * Math.PI);
            cr.clip();
            const f = Math.max(0, Math.min(1, st.frac));
            const top = cy + ri - 2 * ri * f;
            const amp = Math.max(0.6, w * 0.05);
            cr.setSourceRGBA(R, G, B, 0.95);
            cr.moveTo(cx - ri, top);
            for (let i = 0; i <= 20; i++) {
                const x = cx - ri + (2 * ri * i) / 20;
                cr.lineTo(x, top + Math.sin(i / 20 * Math.PI * 2) * amp);
            }
            cr.lineTo(cx + ri, cy + ri + 2);
            cr.lineTo(cx - ri, cy + ri + 2);
            cr.closePath();
            cr.fill();
            cr.restore();
        }
        cr.$dispose();
    });

    return {
        actor: area,
        setBattery(b) {
            const pct = b?.present ? b.percent : 100;
            const charging = !!(b?.present && b.state === 1);
            const full = !!(b?.present && (b.state === 4 || pct >= 100));
            st.mode = 'battery';
            st.frac = pct / 100;
            st.bolt = charging;
            st.rgb = full ? BLUE : (charging || pct > 20 ? GREEN : RED);
            area.queue_repaint();
        },
        setFocus(frac, mode) {
            st.mode = 'focus';
            st.frac = frac;
            st.rgb = mode === 'break' ? BREAK_RGB : FOCUS_RGB;
            st.bolt = false;
            area.queue_repaint();
        },
    };
}
