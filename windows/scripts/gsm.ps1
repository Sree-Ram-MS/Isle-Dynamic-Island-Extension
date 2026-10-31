# Isle — bridge to the Windows media sessions (the same system the
# volume/media overlay uses). Any app that appears there (Spotify,
# Apple Music, browsers playing YouTube, Media Player) shows up here.
#
#   gsm.ps1 -Mode info [-Art 1]   -> one JSON line with the current track
#   gsm.ps1 -Mode cmd -Cmd playpause|play|pause|next|prev|seek -Arg <seconds>
#
# Everything is read from the local session manager; nothing is invented.
# When no app is playing, it answers {"hasSession":false}.

param(
    [string]$Mode = 'info',
    [string]$Cmd = '',
    [double]$Arg = 0,
    [int]$Art = 0
)
$ErrorActionPreference = 'Stop'

try {
    Add-Type -AssemblyName System.Runtime.WindowsRuntime
    $asTaskGeneric = ([System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object {
        $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and
        $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1'
    })[0]
    function AwaitOp($op, $resultType) {
        $task = $asTaskGeneric.MakeGenericMethod($resultType).Invoke($null, @($op))
        $task.Wait(-1) | Out-Null
        return $task.Result
    }

    [Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager, Windows.Media.Control, ContentType = WindowsRuntime] | Out-Null
    [Windows.Media.Control.GlobalSystemMediaTransportControlsSession, Windows.Media.Control, ContentType = WindowsRuntime] | Out-Null
    [Windows.Media.Control.GlobalSystemMediaTransportControlsSessionMediaProperties, Windows.Media.Control, ContentType = WindowsRuntime] | Out-Null

    $mgr = AwaitOp ([Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager]::RequestAsync()) ([Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager])
    $session = $mgr.GetCurrentSession()
    if ($null -eq $session) {
        '{"hasSession":false}'
        exit 0
    }

    if ($Mode -eq 'cmd') {
        $op = $null
        switch ($Cmd) {
            'playpause' { $op = $session.TryTogglePlayPauseAsync() }
            'play'      { $op = $session.TryPlayAsync() }
            'pause'     { $op = $session.TryPauseAsync() }
            'next'      { $op = $session.TrySkipNextAsync() }
            'prev'      { $op = $session.TrySkipPreviousAsync() }
            'seek'      { $op = $session.TryChangePlaybackPositionAsync([int64]($Arg * 10000000)) }
        }
        if ($null -ne $op) { AwaitOp $op ([bool]) | Out-Null }
        '{"ok":true}'
        exit 0
    }

    $props = AwaitOp ($session.TryGetMediaPropertiesAsync()) ([Windows.Media.Control.GlobalSystemMediaTransportControlsSessionMediaProperties])
    $playback = $session.GetPlaybackInfo()
    $timeline = $session.GetTimelineProperties()

    $artB64 = ''
    $artType = ''
    if ($Art -eq 1 -and $null -ne $props.Thumbnail) {
        try {
            [Windows.Storage.Streams.IRandomAccessStreamWithContentType, Windows.Storage.Streams, ContentType = WindowsRuntime] | Out-Null
            [Windows.Storage.Streams.DataReader, Windows.Storage.Streams, ContentType = WindowsRuntime] | Out-Null
            $stream = AwaitOp ($props.Thumbnail.OpenReadAsync()) ([Windows.Storage.Streams.IRandomAccessStreamWithContentType])
            $artType = "$($stream.ContentType)"
            $size = [uint32]$stream.Size
            if ($size -gt 0 -and $size -lt 3000000) {
                $reader = [Windows.Storage.Streams.DataReader]::new($stream.GetInputStreamAt(0))
                AwaitOp ($reader.LoadAsync($size)) ([uint32]) | Out-Null
                $bytes = New-Object byte[] $size
                $reader.ReadBytes($bytes)
                $artB64 = [Convert]::ToBase64String($bytes)
            }
        } catch { $artB64 = '' }
    }

    $result = @{
        hasSession = $true
        title      = "$($props.Title)"
        artist     = "$($props.Artist)"
        album      = "$($props.AlbumTitle)"
        playing    = ("$($playback.PlaybackStatus)" -eq 'Playing')
        status     = "$($playback.PlaybackStatus)"
        position   = $timeline.Position.TotalSeconds
        duration   = ($timeline.EndTime - $timeline.StartTime).TotalSeconds
        art        = $artB64
        artType    = $artType
    }
    $result | ConvertTo-Json -Compress
} catch {
    @{hasSession = $false; error = "$($_.Exception.Message)"} | ConvertTo-Json -Compress
}
