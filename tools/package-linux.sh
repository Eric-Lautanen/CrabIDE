#!/usr/bin/env bash
# Package crabide for Linux: AppImage + .deb + .rpm
# Usage: ./tools/package-linux.sh [target-dir] [version]
set -euo pipefail

TARGET_DIR="${1:-target/release}"
VERSION="${2:-0.1.0}"
BINARY="${TARGET_DIR}/crabide"
APP_NAME="crabide"
DIST_DIR="dist/linux"

if [ ! -f "$BINARY" ]; then
    echo "Error: Binary not found at $BINARY. Run 'cargo build --release' first."
    exit 1
fi

mkdir -p "$DIST_DIR"

# ── AppImage ────────────────────────────────────────────────────────────────
echo "Building AppImage..."
APPDIR="${DIST_DIR}/AppDir"
mkdir -p "${APPDIR}/usr/bin"
mkdir -p "${APPDIR}/usr/share/applications"
mkdir -p "${APPDIR}/usr/share/icons/hicolor/256x256/apps"

# Copy binary
cp "$BINARY" "${APPDIR}/usr/bin/crabide"

# Desktop entry
cat > "${APPDIR}/usr/share/applications/crabide.desktop" << EOF
[Desktop Entry]
Name=crabide Editor
Comment=Resource-efficient, cross-platform code editor
Exec=crabide %F
Icon=crabide
Type=Application
Terminal=false
Categories=Development;TextEditor;
MimeType=text/plain;
StartupNotify=true
StartupWMClass=crabide
EOF

# Icon
if [ -f "assets/icon-256.png" ]; then
    cp "assets/icon-256.png" "${APPDIR}/usr/share/icons/hicolor/256x256/apps/crabide.png"
fi

# AppRun
cat > "${APPDIR}/AppRun" << 'APPRUN'
#!/usr/bin/env bash
HERE="$(dirname "$(readlink -f "$0")")"
export PATH="${HERE}/usr/bin:${PATH}"
export XDG_DATA_DIRS="${HERE}/usr/share:${XDG_DATA_DIRS:-}"
exec "${HERE}/usr/bin/crabide" "$@"
APPRUN
chmod +x "${APPDIR}/AppRun"

# Check for appimagetool
if command -v appimagetool &>/dev/null; then
    appimagetool "${APPDIR}" "${DIST_DIR}/crabide-${VERSION}-x86_64.AppImage"
    echo "Created AppImage: ${DIST_DIR}/crabide-${VERSION}-x86_64.AppImage"
else
    echo "Warning: appimagetool not found. Skipping AppImage creation."
    echo "Install appimagetool from https://github.com/AppImage/AppImageKit/releases"
    # Create a tarball instead
    tar czf "${DIST_DIR}/crabide-${VERSION}-x86_64-linux.tar.gz" -C "${APPDIR}" usr
    echo "Created tarball: ${DIST_DIR}/crabide-${VERSION}-x86_64-linux.tar.gz"
fi

# ── .deb package ────────────────────────────────────────────────────────────
echo "Building .deb package..."
DEB_DIR="${DIST_DIR}/deb"
DEB_PKG_DIR="${DEB_DIR}/crabide_${VERSION}_amd64"
mkdir -p "${DEB_PKG_DIR}/DEBIAN"
mkdir -p "${DEB_PKG_DIR}/usr/bin"
mkdir -p "${DEB_PKG_DIR}/usr/share/applications"
mkdir -p "${DEB_PKG_DIR}/usr/share/icons/hicolor/256x256/apps"
mkdir -p "${DEB_PKG_DIR}/usr/share/doc/crabide"

# Control file
cat > "${DEB_PKG_DIR}/DEBIAN/control" << EOF
Package: crabide
Version: ${VERSION}
Section: editors
Priority: optional
Architecture: amd64
Depends: libc6, libgtk-3-0, libxcb-render0, libxcb-shape0, libxcb-xfixes0, libxkbcommon0, libssl3, libgit2-1.7
Maintainer: crabide Contributors <maintainers@crabide-editor.dev>
Description: Resource-efficient, cross-platform code editor
 crabide is a modern code editor with LSP, DAP, terminal, git,
 and extension support. Built with Rust and egui.
Homepage: https://crabide-editor.dev
EOF

# Copy files
cp "$BINARY" "${DEB_PKG_DIR}/usr/bin/crabide"
cp "${APPDIR}/usr/share/applications/crabide.desktop" "${DEB_PKG_DIR}/usr/share/applications/"

if [ -f "assets/icon-256.png" ]; then
    cp "assets/icon-256.png" "${DEB_PKG_DIR}/usr/share/icons/hicolor/256x256/apps/crabide.png"
fi

# Documentation
cp README.md "${DEB_PKG_DIR}/usr/share/doc/crabide/"
cp LICENSE-MIT "${DEB_PKG_DIR}/usr/share/doc/crabide/" 2>/dev/null || true
cp LICENSE-APACHE "${DEB_PKG_DIR}/usr/share/doc/crabide/" 2>/dev/null || true

# Build .deb
if command -v dpkg-deb &>/dev/null; then
    dpkg-deb --build "${DEB_PKG_DIR}" "${DIST_DIR}/crabide_${VERSION}_amd64.deb"
    echo "Created .deb: ${DIST_DIR}/crabide_${VERSION}_amd64.deb"
else
    echo "Warning: dpkg-deb not found. Skipping .deb creation."
fi

# ── .rpm package ────────────────────────────────────────────────────────────
# Uses rpmbuild if available
echo "Building .rpm package..."
if command -v rpmbuild &>/dev/null; then
    RPM_DIR="${DIST_DIR}/rpm"
    mkdir -p "${RPM_DIR}/SPECS"

    cat > "${RPM_DIR}/SPECS/crabide.spec" << EOF
Name:           crabide
Version:        ${VERSION}
Release:        1%{?dist}
Summary:        Resource-efficient, cross-platform code editor
License:        MIT OR Apache-2.0
URL:            https://crabide-editor.dev
Source0:        %{name}-%{version}.tar.gz
BuildArch:      x86_64
Requires:       libgtk-3, libxcb, libxkbcommon, openssl, libgit2

%description
crabide is a modern code editor with LSP, DAP, terminal, git,
and extension support. Built with Rust and egui.

%prep
%setup -q

%install
mkdir -p %{buildroot}%{_bindir}
mkdir -p %{buildroot}%{_datadir}/applications
mkdir -p %{buildroot}%{_datadir}/icons/hicolor/256x256/apps
install -m 755 target/release/crabide %{buildroot}%{_bindir}/crabide
install -m 644 tools/crabide.desktop %{buildroot}%{_datadir}/applications/crabide.desktop
install -m 644 assets/icon-256.png %{buildroot}%{_datadir}/icons/hicolor/256x256/apps/crabide.png

%files
%{_bindir}/crabide
%{_datadir}/applications/crabide.desktop
%{_datadir}/icons/hicolor/256x256/apps/crabide.png

%changelog
* $(date '+%a %b %d %Y') crabide Contributors <maintainers@crabide-editor.dev> - ${VERSION}
- Initial release
EOF

    # Build RPM
    rpmbuild --define "_topdir ${RPM_DIR}" -bb "${RPM_DIR}/SPECS/crabide.spec" \
        --define "_rpmdir ${DIST_DIR}"
    echo "Created .rpm package"
else
    echo "Warning: rpmbuild not found. Skipping .rpm creation."
fi

echo "Linux packaging complete. Output in ${DIST_DIR}/"
