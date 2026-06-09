#!/usr/bin/env bash
# Package crabide for macOS: create .app bundle + optional DMG
# Usage: ./tools/package-macos.sh [target-dir] [version]
set -euo pipefail

TARGET_DIR="${1:-target/release}"
VERSION="${2:-0.1.0}"
BINARY="${TARGET_DIR}/crabide"
APP_NAME="crabide.app"
APP_DIR="dist/macos/${APP_NAME}"
DMG_NAME="crabide-${VERSION}-aarch64-macos.dmg"
DIST_DIR="dist/macos"

if [ ! -f "$BINARY" ]; then
    echo "Error: Binary not found at $BINARY. Run 'cargo build --release' first."
    exit 1
fi

# Create .app bundle structure
mkdir -p "${APP_DIR}/Contents/MacOS"
mkdir -p "${APP_DIR}/Contents/Resources"

# Copy binary
cp "$BINARY" "${APP_DIR}/Contents/MacOS/crabide"

# Create Info.plist
cat > "${APP_DIR}/Contents/Info.plist" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>crabide</string>
    <key>CFBundleIdentifier</key>
    <string>dev.crabide.editor</string>
    <key>CFBundleName</key>
    <string>crabide</string>
    <key>CFBundleDisplayName</key>
    <string>crabide Editor</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleIconFile</key>
    <string>icon.icns</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>LSUIElement</key>
    <false/>
    <key>NSHumanReadableCopyright</key>
    <string>MIT OR Apache-2.0</string>
</dict>
</plist>
EOF

# Copy icons
if [ -f "assets/icon.icns" ]; then
    cp "assets/icon.icns" "${APP_DIR}/Contents/Resources/icon.icns"
fi

# Copy docs
cp README.md "${APP_DIR}/Contents/Resources/"
cp LICENSE-MIT "${APP_DIR}/Contents/Resources/" 2>/dev/null || true
cp LICENSE-APACHE "${APP_DIR}/Contents/Resources/" 2>/dev/null || true

# Sign the app bundle (if certificate is available)
if command -v codesign &>/dev/null && security find-identity -v -p basic 2>/dev/null | grep -q "Developer ID"; then
    echo "Signing application bundle..."
    codesign --force --deep --sign - "${APP_DIR}"
fi

# Create DMG (if hdiutil is available)
if command -v hdiutil &>/dev/null; then
    echo "Creating DMG..."
    mkdir -p "$DIST_DIR"
    hdiutil create -volname "crabide" \
        -srcfolder "${APP_DIR}" \
        -ov -format UDZO \
        "${DIST_DIR}/${DMG_NAME}"
    echo "Created DMG: ${DIST_DIR}/${DMG_NAME}"
fi

echo "macOS packaging complete: ${APP_DIR}"
