#!/bin/sh
# The glass host against the installed daemon: wraps tools/glasshost.m as an app bundle, opens it,
# and lets the daemon load the installed ErisMenus into it at launch. The host waits until the
# payload's glass is in, runs the scenarios and writes what it saw to the output folder (out.txt,
# plus a PNG of each window as the window server composited it).
#
#   tools/glass-e2e.sh [out-dir] [scenario ...]
set -eu
cd "$(dirname "$0")/.."
out=${1:-/tmp/erismenus-glass-e2e}
[ $# -gt 0 ] && shift
app=target/e2e/ErisMenusGlassTest.app
mkdir -p "$app/Contents/MacOS" "$out"
clang -fobjc-arc -framework AppKit -framework QuartzCore tools/glasshost.m -o "$app/Contents/MacOS/ErisMenusGlassTest"
cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>ErisMenusGlassTest</string>
<key>CFBundleIdentifier</key><string>io.github.eriskii.erismenus.glasstest</string>
<key>CFBundleName</key><string>ErisMenusGlassTest</string>
<key>CFBundlePackageType</key><string>APPL</string>
</dict></plist>
PLIST
codesign -s - --force "$app" 2>/dev/null
rm -f "$out/out.txt" "$out/err.txt"
open -n -W --stdout "$out/out.txt" --stderr "$out/err.txt" "$app" --args --injected "$out" "$@" || true
cat "$out/out.txt"
# Before an install with glass in it, this says the glass never loaded.
grep -q "all checks passed" "$out/out.txt"
