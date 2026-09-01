#!/bin/bash
# Assembles ClaudePet.app from the SwiftPM build output - no Xcode project needed.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

APP_NAME="ClaudePet"
BUNDLE_ID="com.emm312.claudepet"
APP_DIR="$ROOT_DIR/$APP_NAME.app"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"

echo "==> Building (release, arm64)"
swift build -c release --arch arm64
ARM_BIN="$(swift build -c release --arch arm64 --show-bin-path)/$APP_NAME"

echo "==> Building (release, x86_64)"
swift build -c release --arch x86_64
X86_BIN="$(swift build -c release --arch x86_64 --show-bin-path)/$APP_NAME"

echo "==> Assembling $APP_NAME.app"
rm -rf "$APP_DIR"
mkdir -p "$MACOS_DIR" "$RESOURCES_DIR"

echo "==> Combining into universal binary"
lipo -create -output "$MACOS_DIR/$APP_NAME" "$ARM_BIN" "$X86_BIN"

cat > "$CONTENTS_DIR/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>$APP_NAME</string>
    <key>CFBundleIdentifier</key>
    <string>$BUNDLE_ID</string>
    <key>CFBundleName</key>
    <string>$APP_NAME</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>LSUIElement</key>
    <true/>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>LSMinimumSystemVersion</key>
    <string>13.0</string>
    <key>NSSupportsAutomaticGraphicsSwitching</key>
    <true/>
    <key>NSLocalNetworkUsageDescription</key>
    <string>ClaudePet uses your local network to find nearby pets to deliver messages to.</string>
    <key>NSBluetoothAlwaysUsageDescription</key>
    <string>ClaudePet uses Bluetooth to find nearby pets to deliver messages to.</string>
    <key>NSBonjourServices</key>
    <array>
        <string>_claudepet._tcp</string>
        <string>_claudepet._udp</string>
    </array>
</dict>
</plist>
PLIST

echo "==> Ad-hoc code signing"
codesign --force --deep --sign - "$APP_DIR"

echo "==> Done: $APP_DIR"
echo "Run with: open \"$APP_DIR\""
