# Isle — small Windows helpers the notch needs from the main process.
#
#   win.ps1 -Mode volget                 -> {"volume":0..100,"muted":bool}
#   win.ps1 -Mode volset -Arg <0..100>   -> sets the master output volume
#   win.ps1 -Mode fg                     -> {"fullscreen":bool} for the
#                                            foreground window (games etc.)

param(
    [string]$Mode = '',
    [double]$Arg = 0
)
$ErrorActionPreference = 'Stop'

try {
    if ($Mode -eq 'volget' -or $Mode -eq 'volset') {
        Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

[Guid("BCDE0395-E52F-467C-8E3D-C4579291692E")]
class MMDeviceEnumeratorComObject { }

[InterfaceType(ComInterfaceType.InterfaceIsIUnknown), Guid("A95664D2-9614-4F35-A746-DE8DB63617E6")]
interface IMMDeviceEnumerator {
    int NotImpl1([MarshalAs(UnmanagedType.IUnknown)] object o);
    int GetDefaultAudioEndpoint(int dataFlow, int role, out IMMDevice device);
}

[InterfaceType(ComInterfaceType.InterfaceIsIUnknown), Guid("5CDF2C82-841E-4546-9722-0CF74078229A")]
interface IMMDevice {
    int Activate(ref Guid iid, int clsCtx, IntPtr activationParams,
        [MarshalAs(UnmanagedType.IUnknown)] out object instance);
}

[InterfaceType(ComInterfaceType.InterfaceIsIUnknown), Guid("C02216F6-8C67-4B5B-9D00-D008E73E0064")]
interface IAudioEndpointVolume {
    int RegisterControlChangeNotify(IntPtr notify);
    int UnregisterControlChangeNotify(IntPtr notify);
    int GetChannelCount(out int count);
    int SetMasterVolumeLevel(float level, ref Guid eventContext);
    int SetMasterVolumeLevelScalar(float level, ref Guid eventContext);
    int GetMasterVolumeLevel(out float level);
    int GetMasterVolumeLevelScalar(out float level);
    int GetVolumeRange(out float minDb, out float maxDb, out float incrementDb);
    int GetMute(out bool mute);
    int SetMute(bool mute, ref Guid eventContext);
}

public static class IsleAudio {
    static IAudioEndpointVolume Endpoint() {
        var enumerator = (IMMDeviceEnumerator)(new MMDeviceEnumeratorComObject());
        IMMDevice device;
        Marshal.ThrowExceptionForHR(enumerator.GetDefaultAudioEndpoint(0, 1, out device));
        Guid iid = typeof(IAudioEndpointVolume).GUID;
        object instance;
        Marshal.ThrowExceptionForHR(device.Activate(ref iid, 23, IntPtr.Zero, out instance));
        return (IAudioEndpointVolume)instance;
    }
    public static float GetVolume() {
        float v; Marshal.ThrowExceptionForHR(Endpoint().GetMasterVolumeLevelScalar(out v)); return v;
    }
    public static bool GetMute() {
        bool m; Marshal.ThrowExceptionForHR(Endpoint().GetMute(out m)); return m;
    }
    public static void SetVolume(float v) {
        Guid g = Guid.Empty;
        Marshal.ThrowExceptionForHR(Endpoint().SetMasterVolumeLevelScalar(v, ref g));
    }
}
'@
        if ($Mode -eq 'volset') {
            [IsleAudio]::SetVolume([float]($Arg / 100.0))
            '{"ok":true}'
        } else {
            @{volume = [math]::Round([IsleAudio]::GetVolume() * 100); muted = [IsleAudio]::GetMute()} |
                ConvertTo-Json -Compress
        }
        exit 0
    }

    if ($Mode -eq 'fg') {
        Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

public static class IsleFg {
    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] static extern IntPtr MonitorFromWindow(IntPtr h, uint flags);
    [DllImport("user32.dll")] static extern bool GetMonitorInfo(IntPtr m, ref MONITORINFO info);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr h, System.Text.StringBuilder s, int max);
    struct RECT { public int Left, Top, Right, Bottom; }
    struct MONITORINFO { public int Size; public RECT Monitor; public RECT WorkArea; public uint Flags; }

    public static bool ForegroundIsFullscreen() {
        IntPtr h = GetForegroundWindow();
        if (h == IntPtr.Zero) return false;
        var cls = new System.Text.StringBuilder(64);
        GetClassName(h, cls, 64);
        string name = cls.ToString();
        if (name == "Progman" || name == "WorkerW" || name == "Shell_TrayWnd") return false;
        RECT r;
        if (!GetWindowRect(h, out r)) return false;
        IntPtr mon = MonitorFromWindow(h, 2);
        var info = new MONITORINFO(); info.Size = Marshal.SizeOf(typeof(MONITORINFO));
        if (!GetMonitorInfo(mon, ref info)) return false;
        return r.Left <= info.Monitor.Left && r.Top <= info.Monitor.Top &&
               r.Right >= info.Monitor.Right && r.Bottom >= info.Monitor.Bottom;
    }
}
'@
        @{fullscreen = [IsleFg]::ForegroundIsFullscreen()} | ConvertTo-Json -Compress
        exit 0
    }

    '{"error":"unknown mode"}'
} catch {
    @{error = "$($_.Exception.Message)"} | ConvertTo-Json -Compress
}
