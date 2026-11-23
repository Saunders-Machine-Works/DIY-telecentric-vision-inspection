#!/bin/sh
# Build "Thread Vision.app", a double-clickable macOS launcher for the release app in this
# repository, with its own icon. Usage, from anywhere:
#   tools/mac/make_launcher.sh [destination folder]      (default: ~/Applications)
# The launcher starts target/release/app from this repo, so the app finds captures/ and
# writes measurements.csv here. Rebuild the app with ./run_app.sh or cargo after pulling;
# re-run this script only if the repository moves. Needs python3 with Pillow for the icon.
set -e
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
DEST="${1:-$HOME/Applications}"
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
  <key>CFBundleExecutable</key><string>launch</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
EOF

cat > "$APP/Contents/MacOS/launch" <<EOF
#!/bin/sh
BIN="$REPO/target/release/app"
if [ ! -x "\$BIN" ]; then
  osascript -e 'display alert "Thread Vision is not built" message "Run ./run_app.sh once in $REPO, then try again."'
  exit 1
fi
exec "\$BIN"
EOF
chmod +x "$APP/Contents/MacOS/launch"

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
touch "$APP"
echo "Built $APP"
