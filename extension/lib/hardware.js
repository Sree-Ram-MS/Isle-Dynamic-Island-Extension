import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

Gio._promisify(Gio.Subprocess.prototype, 'communicate_utf8_async', 'communicate_utf8_finish');

const dec = new TextDecoder();

function readText(path) {
    try {
        const [ok, bytes] = GLib.file_get_contents(path);
        return ok ? dec.decode(bytes) : null;
    } catch (_e) {
        return null;
    }
}

export function fmtRate(bytesPerSec) {
    if (bytesPerSec >= 1e9) return `${(bytesPerSec / 1e9).toFixed(1)} GB/s`;
    if (bytesPerSec >= 1e6) return `${(bytesPerSec / 1e6).toFixed(1)} MB/s`;
    if (bytesPerSec >= 1e3) return `${(bytesPerSec / 1e3).toFixed(1)} KB/s`;
    return `${Math.round(bytesPerSec)} B/s`;
}

// CPU, RAM, GPU, network and disk readings from /proc, /sys and statvfs.
// GPU: NVIDIA (nvidia-smi), AMD (gpu_busy_percent) or, for Intel, current/maximum clock as a rough load.
export class HardwareWatcher {
    constructor() {
        this.cpu = 0;
        this.ramUsed = 0;
        this.ramTotal = 0;
        this.disk = 0;
        this.diskUsed = 0;    // bytes on /
        this.diskTotal = 0;   // bytes on /
        this.gpu = null;      // 0..100 or null when no sensor is available
        this.gpuApprox = false;
        this.up = 0;          // bytes per second
        this.down = 0;
        this._cpuPrev = null;
        this._netPrev = null;
        this._gpuProbe = undefined;
        this._gpuBusy = false;
    }

    poll() {
        this._cpuRead();
        this._ramRead();
        this._netRead();
        this._diskRead();
        this._gpuRead();
    }

    _cpuRead() {
        const t = readText('/proc/stat');
        if (!t) return;
        const f = t.split('\n', 1)[0].trim().split(/\s+/).slice(1, 9).map(Number);
        const idle = f[3] + (f[4] || 0);
        const total = f.reduce((a, b) => a + b, 0);
        if (this._cpuPrev) {
            const dt = total - this._cpuPrev.total;
            const di = idle - this._cpuPrev.idle;
            if (dt > 0) this.cpu = Math.max(0, Math.min(100, Math.round(100 * (1 - di / dt))));
        }
        this._cpuPrev = {total, idle};
    }

    _ramRead() {
        const t = readText('/proc/meminfo');
        if (!t) return;
        const get = k => {
            const m = new RegExp(`^${k}:\\s+(\\d+)`, 'm').exec(t);
            return m ? Number(m[1]) * 1024 : 0;
        };
        this.ramTotal = get('MemTotal');
        this.ramUsed = Math.max(0, this.ramTotal - get('MemAvailable'));
    }

    _netRead() {
        const t = readText('/proc/net/dev');
        if (!t) return;
        let rx = 0;
        let tx = 0;
        for (const line of t.split('\n').slice(2)) {
            const [name, rest] = line.split(':');
            if (!rest) continue;
            const n = name.trim();
            if (n === 'lo' || /^(docker|veth|br-|virbr|tun|tap)/.test(n)) continue;
            const f = rest.trim().split(/\s+/).map(Number);
            rx += f[0];
            tx += f[8];
        }
        const now = GLib.get_monotonic_time() / 1e6;
        if (this._netPrev) {
            const dt = now - this._netPrev.t;
            if (dt > 0) {
                this.down = Math.max(0, (rx - this._netPrev.rx) / dt);
                this.up = Math.max(0, (tx - this._netPrev.tx) / dt);
            }
        }
        this._netPrev = {t: now, rx, tx};
    }

    _diskRead() {
        try {
            const info = Gio.File.new_for_path('/').query_filesystem_info('filesystem::size,filesystem::free', null);
            const size = info.get_attribute_uint64('filesystem::size');
            const free = info.get_attribute_uint64('filesystem::free');
            if (size > 0) {
                this.disk = Math.round(100 * (size - free) / size);
                this.diskTotal = size;
                this.diskUsed = size - free;
            }
        } catch (_e) { /* keep the last value */ }
    }

    // find the first usable GPU sensor once
    _probeGpu() {
        if (GLib.find_program_in_path('nvidia-smi')) return {kind: 'nvidia'};
        try {
            const dir = Gio.File.new_for_path('/sys/class/drm');
            const en = dir.enumerate_children('standard::name', Gio.FileQueryInfoFlags.NONE, null);
            let info;
            while ((info = en.next_file(null))) {
                const name = info.get_name();
                if (!/^card\d+$/.test(name)) continue;
                const base = `/sys/class/drm/${name}`;
                if (readText(`${base}/device/gpu_busy_percent`) !== null)
                    return {kind: 'amd', path: `${base}/device/gpu_busy_percent`};
                if (readText(`${base}/gt_act_freq_mhz`) !== null && readText(`${base}/gt_RP0_freq_mhz`) !== null)
                    return {kind: 'intel', cur: `${base}/gt_act_freq_mhz`, max: `${base}/gt_RP0_freq_mhz`};
                const xe = `${base}/device/tile0/gt0/freq0`;
                if (readText(`${xe}/act_freq`) !== null && readText(`${xe}/max_freq`) !== null)
                    return {kind: 'intel', cur: `${xe}/act_freq`, max: `${xe}/max_freq`};
            }
        } catch (_e) { /* no DRM */ }
        return {kind: 'none'};
    }

    _gpuRead() {
        if (this._gpuProbe === undefined) this._gpuProbe = this._probeGpu();
        const p = this._gpuProbe;
        if (p.kind === 'amd') {
            const v = parseInt(readText(p.path), 10);
            if (!Number.isNaN(v)) this.gpu = v;
        } else if (p.kind === 'intel') {
            const cur = parseInt(readText(p.cur), 10);
            const max = parseInt(readText(p.max), 10);
            if (max > 0 && !Number.isNaN(cur)) {
                this.gpu = Math.max(0, Math.min(100, Math.round(100 * cur / max)));
                this.gpuApprox = true;
            }
        } else if (p.kind === 'nvidia' && !this._gpuBusy) {
            this._gpuBusy = true;
            this._nvidia().finally(() => { this._gpuBusy = false; });
        }
    }

    async _nvidia() {
        try {
            const proc = Gio.Subprocess.new(
                ['nvidia-smi', '--query-gpu=utilization.gpu', '--format=csv,noheader,nounits'],
                Gio.SubprocessFlags.STDOUT_PIPE | Gio.SubprocessFlags.STDERR_SILENCE);
            const [out] = await proc.communicate_utf8_async(null, null);
            const v = parseInt(out, 10);
            if (!Number.isNaN(v)) this.gpu = v;
        } catch (_e) { /* driver not ready */ }
    }
}
