# Builds the Stream Deck plugin and installs it, restarting Stream Deck so it loads.
$root = $PSScriptRoot
# Before "Stop": Windows PowerShell treats cargo's progress output on stderr as an error.
cargo build --release --manifest-path "$root\Cargo.toml"
if ($LASTEXITCODE) { throw "build failed" }
$ErrorActionPreference = "Stop"

$plugin = "$root\com.resonance.streamdeck.sdPlugin"
Copy-Item "$root\target\release\resonance-streamdeck.exe" $plugin -Force

$deck = Get-Process StreamDeck -ErrorAction SilentlyContinue
$deckExe = if ($deck) { $deck.Path } else { "C:\Program Files\Elgato\StreamDeck\StreamDeck.exe" }
if ($deck) { $deck | Stop-Process; $deck | Wait-Process -Timeout 10 }
Get-Process resonance-streamdeck -ErrorAction SilentlyContinue | Stop-Process

$dest = "$env:APPDATA\Elgato\StreamDeck\Plugins\com.resonance.streamdeck.sdPlugin"
if (Test-Path $dest) { Remove-Item $dest -Recurse -Force }
Copy-Item $plugin $dest -Recurse
Start-Process $deckExe
"Installed to $dest"
