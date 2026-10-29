import Adw from 'gi://Adw';
import Gio from 'gi://Gio';
import Gtk from 'gi://Gtk';

import {ExtensionPreferences} from 'resource:///org/gnome/Shell/Extensions/js/extensions/prefs.js';

export default class NusaNotchPrefs extends ExtensionPreferences {
    fillPreferencesWindow(window) {
        const s = this.getSettings();
        window.set_default_size(560, 720);
        const page = new Adw.PreferencesPage({title: 'Nusa Notch', icon_name: 'preferences-system-symbolic'});
        window.add(page);

        const group = (title, description = '') => {
            const g = new Adw.PreferencesGroup({title, description});
            page.add(g);
            return g;
        };
        const sw = (g, key, title, subtitle = '') => {
            const row = new Adw.SwitchRow({title, subtitle});
            s.bind(key, row, 'active', Gio.SettingsBindFlags.DEFAULT);
            g.add(row);
            return row;
        };
        const spin = (g, key, title, subtitle, lo, hi, step, digits = 0) => {
            const row = new Adw.SpinRow({
                title, subtitle, digits,
                adjustment: new Gtk.Adjustment({lower: lo, upper: hi, step_increment: step, page_increment: step * 5}),
            });
            s.bind(key, row.adjustment, 'value', Gio.SettingsBindFlags.DEFAULT);
            g.add(row);
        };

        const look = group('Notch');
        sw(look, 'mascot', 'Mascot', 'Show a pair of eyes when the computer is idle');
        sw(look, 'minimal-notch', 'Minimal notch', 'A thin line when closed (for laptops with thick bezels)');
        sw(look, 'hide-in-fullscreen', 'Hide in full screen', 'Step aside for full-screen apps');
        spin(look, 'notch-scale', 'Notch size', '0.85 is the default size', 0.6, 1.6, 0.05, 2);
        sw(look, 'status-peeks', 'Status peeks', 'Battery, Wi-Fi, headphones and Caps Lock events');

        const music = group('Music', 'Playback comes from any MPRIS player (Spotify, browsers, VLC, Rhythmbox, …).');
        sw(music, 'lyrics-enabled', 'Synced lyrics', 'Sends song title, artist, album and length to lrclib.net');

        const focus = group('Focus timer');
        spin(focus, 'focus-minutes', 'Focus length', 'Minutes', 1, 180, 1);
        spin(focus, 'break-minutes', 'Break length', 'Minutes', 1, 60, 1);

        const water = group('Water reminder');
        spin(water, 'water-goal-ml', 'Daily goal', 'Millilitres', 500, 6000, 100);
        spin(water, 'water-bottle-ml', 'Bottle size', 'Added with each Drink', 50, 2000, 50);
        sw(water, 'water-reminders', 'Reminders', 'Peek in the notch while you are below your goal');
        spin(water, 'water-interval-min', 'Every', 'Minutes between reminders (reminder hours are set in the notch: Water > Settings)', 5, 240, 5);

        const about = group('About');
        about.add(new Adw.ActionRow({
            title: 'Unofficial Linux clone',
            subtitle: 'Inspired by Nusa for Windows. Tasks, water logs and shelf items are stored locally in ~/.local/share/nusa-notch. No accounts, no analytics.',
        }));
    }
}
