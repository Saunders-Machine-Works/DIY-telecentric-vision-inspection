#!/bin/sh
# Build "Thread Vision.app" in /Applications (or the folder given) from this repository's
# release build, with its own icon. ./run_app.sh calls this after every build, so the bundle
# always holds the current app. Usage: tools/mac/make_launcher.sh [destination folder]
#
# The real app binary is copied INTO the bundle: macOS Local Network privacy (needed to reach
# the GigE camera) is granted per app, and it only asks for, and remembers, a proper bundle
# whose own executable makes the connection. A script that runs the repo binary is blocked
# silently, with no prompt. Allow the prompt on first launch, or later in System Settings >
# Privacy & Security > Local Network. THREAD_VISION_DATA (LSEnvironment) points the app at
# this repo, so it still reads captures/calibration.txt and writes measurements.csv here.
# Needs python3 with Pillow for the icon.
set -e
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
DEST="${1:-/Applications}"
APP="$DEST/Thread Vision.app"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$DEST"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Thread Vision</string>
  <key>CFBundleDisplayName</key><string>Thread Vision</string>
  <key>CFBundleIdentifier</key><string>local.threadvision.launcher</string>
  <key>CFBundleExecutable</key><string>ThreadVision</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSEnvironment</key><dict><key>THREAD_VISION_DATA</key><string>$REPO</string></dict>
  <key>NSLocalNetworkUsageDescription</key><string>Thread Vision talks to the Basler GigE camera on the local network.</string>
</dict></plist>
EOF

BIN="$REPO/target/release/app"
if [ ! -x "$BIN" ]; then
  echo "No release build at $BIN: run ./run_app.sh (or cargo build --release -p app) first." >&2
  exit 1
fi
cp "$BIN" "$APP/Contents/MacOS/ThreadVision"

# Icon: a backlit pin silhouette between two edge marks, drawn at 1024 px.
python3 - "$TMP/icon.png" <<'EOF'
import sys
from PIL import Image, ImageDraw
S = 1024
im = Image.new("RGBA", (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(im)
d.rounded_rectangle([64, 64, S - 64, S - 64], radius=200, fill=(232, 236, 240, 255))
d.rounded_rectangle([392, 150, 632, 874], radius=40, fill=(28, 30, 34, 255))
amber = (240, 170, 40, 255)
for x in (360, 664):
    d.rectangle([x - 10, 230, x + 10, 794], fill=amber)
d.rectangle([250, 502, 774, 522], fill=amber)
im.save(sys.argv[1])
EOF
SET="$TMP/AppIcon.iconset"
mkdir -p "$SET"
for s in 16 32 128 256 512; do
  sips -z $s $s "$TMP/icon.png" --out "$SET/icon_${s}x${s}.png" >/dev/null
  d=$((s * 2))
  sips -z $d $d "$TMP/icon.png" --out "$SET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$SET" -o "$APP/Contents/Resources/AppIcon.icns"
# Ad-hoc signature, so the bundle has an identity for its privacy permission.
codesign --force --sign - "$APP"
touch "$APP"
echo "Built $APP"
