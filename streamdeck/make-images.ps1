# Renders icons/*.svg into com.resonance.streamdeck.sdPlugin/imgs (using Edge headless), and copies
# Resonance's app icon in as the plugin icon. Run again after editing an SVG.
Add-Type -AssemblyName System.Drawing
$edge = "C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"
$src = Join-Path $PSScriptRoot "icons"
$imgs = Join-Path $PSScriptRoot "com.resonance.streamdeck.sdPlugin\imgs"
$work = Join-Path ([IO.Path]::GetTempPath()) "resonance-streamdeck-icons"
New-Item -ItemType Directory -Force $imgs, $work | Out-Null

function Render($svg, $out, $px) {
    $page = Join-Path $work "page.html"
    $markup = Get-Content (Join-Path $src $svg) -Raw
    Set-Content $page "<html><body style='margin:0'><div style='width:${px}px;height:${px}px'>$($markup -replace '<svg ', '<svg width=''100%'' height=''100%'' ')</div></body></html>" -Encoding utf8
    $png = Join-Path $work "shot.png"
    if (Test-Path $png) { Remove-Item $png }
    & $edge --headless=new --disable-gpu --hide-scrollbars --force-device-scale-factor=1 `
        --default-background-color=00000000 --window-size=$px,$px --screenshot=$png "file:///$($page -replace '\\', '/')" 2>$null | Out-Null
    for ($i = 0; -not (Test-Path $png) -and $i -lt 50; $i++) { Start-Sleep -Milliseconds 100 }
    Move-Item $png (Join-Path $imgs $out) -Force
}

foreach ($state in "mic-live", "mic-muted", "mic-offline") {
    Render "$state.svg" "$state.png" 72
    Render "$state.svg" "$state@2x.png" 144
}
Render "mono.svg" "action.png" 20
Render "mono.svg" "action@2x.png" 40
Render "mono.svg" "category.png" 28
Render "mono.svg" "category@2x.png" 56

$icon = [System.Drawing.Image]::FromFile((Resolve-Path (Join-Path $PSScriptRoot "..\src-tauri\icons\icon.png")))
foreach ($s in @(@{ n = "plugin.png"; px = 256 }, @{ n = "plugin@2x.png"; px = 512 })) {
    $bmp = New-Object System.Drawing.Bitmap $s.px, $s.px
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.InterpolationMode = "HighQualityBicubic"
    $g.DrawImage($icon, 0, 0, $s.px, $s.px)
    $bmp.Save((Join-Path $imgs $s.n), [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
}
$icon.Dispose()
"images written to $imgs"
