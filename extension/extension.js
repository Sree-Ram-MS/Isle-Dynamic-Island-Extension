import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

import {Notch} from './notch.js';

export default class IsleExtension extends Extension {
    enable() {
        this._notch = new Notch(this);
        this._notch.enable();
    }

    disable() {
        this._notch?.destroy();
        this._notch = null;
    }
}
