import Gio from 'gi://Gio';

Gio._promisify(Gio.Subprocess.prototype, 'communicate_utf8_async', 'communicate_utf8_finish');

// Real system output volume (PipeWire via wpctl, PulseAudio via pactl as fallback).
// Browsers and many players ignore the MPRIS Volume property, so this is what actually changes the sound.
export class SystemVolume {
    constructor() {
        this._busy = false;
        this._pending = null;
        this._backend = null; // 'wpctl' | 'pactl'
    }

    async _run(argv) {
        const p = new Gio.Subprocess({argv, flags: Gio.SubprocessFlags.STDOUT_PIPE | Gio.SubprocessFlags.STDERR_SILENCE});
        p.init(null);
        const [out] = await p.communicate_utf8_async(null, null);
        if (!p.get_successful()) throw new Error(`${argv[0]} failed`);
        return out ?? '';
    }

    // -> 0..1 (0 when muted) or null
    async get() {
        for (const be of this._backend ? [this._backend] : ['wpctl', 'pactl']) {
            try {
                if (be === 'wpctl') {
                    const out = await this._run(['wpctl', 'get-volume', '@DEFAULT_AUDIO_SINK@']);
                    const m = /Volume:\s*([\d.]+)/.exec(out);
                    if (!m) continue;
                    this._backend = 'wpctl';
                    return out.includes('[MUTED]') ? 0 : Math.min(1, parseFloat(m[1]));
                }
                const out = await this._run(['pactl', 'get-sink-volume', '@DEFAULT_SINK@']);
                const m = /(\d+)%/.exec(out);
                if (!m) continue;
                this._backend = 'pactl';
                return Math.min(1, parseInt(m[1], 10) / 100);
            } catch (_e) {
                // try next backend
            }
        }
        return null;
    }

    // Coalesces rapid calls (slider drag / scrolling) so only the latest value is applied.
    set(v) {
        this._pending = Math.max(0, Math.min(1, v));
        if (!this._busy) this._flush();
    }

    async _flush() {
        this._busy = true;
        while (this._pending !== null) {
            const v = this._pending;
            this._pending = null;
            try {
                if (!this._backend) await this.get();
                if (this._backend === 'wpctl') {
                    if (v > 0) await this._run(['wpctl', 'set-mute', '@DEFAULT_AUDIO_SINK@', '0']).catch(() => {});
                    await this._run(['wpctl', 'set-volume', '@DEFAULT_AUDIO_SINK@', v.toFixed(2)]);
                } else if (this._backend === 'pactl') {
                    await this._run(['pactl', 'set-sink-volume', '@DEFAULT_SINK@', `${Math.round(v * 100)}%`]);
                }
            } catch (e) {
                console.error(`[nusa-notch] volume: ${e.message}`);
            }
        }
        this._busy = false;
    }
}
