#!/bin/sh
# End-to-end test against the installed daemon: wraps the development host as an app bundle, opens
# it, and lets the daemon load the installed ErisMenus into it at launch, as it does for every app.
# The host waits until it's been loaded, runs the scenarios and writes what it saw to the output
# folder (out.txt, plus a PNG of each menu).
#
#   tools/e2e.sh [out-dir] [scenario ...]
set -eu
cd "$(dirname "$0")/.."
out=${1:-/tmp/erismenus-e2e}
[ $# -gt 0 ] && shift
app=target/e2e/ErisMenusTest.app
mkdir -p "$app/Contents/MacOS" "$out"
clang -fobjc-arc -framework AppKit -framework QuartzCore tools/devhost.m -o "$app/Contents/MacOS/ErisMenusTest"
cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>ErisMenusTest</string>
<key>CFBundleIdentifier</key><string>io.github.eriskii.erismenus.test</string>
<key>CFBundleName</key><string>ErisMenusTest</string>
<key>CFBundlePackageType</key><string>APPL</string>
</dict></plist>
PLIST
codesign -s - --force "$app" 2>/dev/null
rm -f "$out/out.txt" "$out/err.txt"
open -n -W --stdout "$out/out.txt" --stderr "$out/err.txt" "$app" --args --injected "$out" "$@"
grep -v willHighlight "$out/out.txt"
