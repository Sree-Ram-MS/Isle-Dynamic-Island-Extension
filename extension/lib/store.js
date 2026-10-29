import GLib from 'gi://GLib';

// Tiny JSON store: ~/.local/share/nusa-notch/data.json
// Tasks, water logs and shelf items live here. No accounts, no analytics.
export class Store {
    constructor() {
        this._dir = GLib.build_filenamev([GLib.get_user_data_dir(), 'nusa-notch']);
        this._file = GLib.build_filenamev([this._dir, 'data.json']);
        this.data = {tasks: [], water: {}, shelf: [], reminders: [], nextId: 1};
        this._saveId = 0;
        this._load();
    }

    _load() {
        try {
            const [ok, bytes] = GLib.file_get_contents(this._file);
            if (!ok) return;
            const parsed = JSON.parse(new TextDecoder().decode(bytes));
            this.data = Object.assign(this.data, parsed);
        } catch (_e) {
            // first run or unreadable file: start empty
        }
    }

    nextId() {
        return this.data.nextId++;
    }

    save() {
        if (this._saveId) return;
        this._saveId = GLib.timeout_add(GLib.PRIORITY_LOW, 400, () => {
            this._saveId = 0;
            this._write();
            return GLib.SOURCE_REMOVE;
        });
    }

    _write() {
        try {
            GLib.mkdir_with_parents(this._dir, 0o755);
            GLib.file_set_contents(this._file, JSON.stringify(this.data));
        } catch (e) {
            console.error(`[nusa-notch] could not save data: ${e.message}`);
        }
    }

    destroy() {
        if (this._saveId) {
            GLib.source_remove(this._saveId);
            this._saveId = 0;
            this._write();
        }
    }
}
