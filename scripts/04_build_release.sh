#!/usr/bin/env bash
# Fedora/Linux native binary + optional RPM packaging. Does not regenerate sources.
# Windows EXE / installer: run 04_build_release.ps1 on Windows instead.
set -euo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/_common.sh"
if [[ "${1:-}" == '--help' ]]; then
  cat <<'HELP'
Usage: scripts/04_build_release.sh [--rpm-only]
Default: build native Linux release binaries and attempt an RPM if rpmbuild is installed.
--rpm-only: use already-built native Linux binaries; requires rpmbuild.
Windows: scripts/04_build_release.ps1 on a Windows MSVC Rust toolchain.
Flatpak: existing manifest is experimental; not built by this script.
HELP
  exit 0
fi
[[ "${1:-}" == '' || "${1:-}" == '--rpm-only' ]] || fail 'Unexpected argument; try --help'
verify_root
[[ "$(uname -s)" == 'Linux' ]] || fail 'Linux release script requires Linux; use scripts/04_build_release.ps1 on Windows'
cd -- "$FPECS_ROOT"
LINUX_OUT="$FPECS_ROOT/dist/linux-x86_64"
mkdir -p "$LINUX_OUT"
BINS=(desktop-gui sensor-simulator telemetry-receiver telemetry-distributor)
if [[ "${1:-}" != '--rpm-only' ]]; then
  need cargo
  info 'Compiling native Linux release from existing source'
  cargo build --release --workspace
fi
for bin in "${BINS[@]}"; do
  [[ -x "target/release/$bin" ]] || fail "Missing executable target/release/$bin (build the workspace first)"
  install -m 0755 "target/release/$bin" "$LINUX_OUT/$bin"
done
info 'Linux executable binaries copied into dist/linux-x86_64/ (native binaries, not installers).'
if ! command -v rpmbuild >/dev/null 2>&1; then
  if [[ "${1:-}" == '--rpm-only' ]]; then fail 'rpmbuild is missing. Install rpm-build on Fedora.'; fi
  info 'SKIP RPM: rpmbuild not installed (Fedora: sudo dnf install rpm-build).'
  exit 0
fi
[[ "$(uname -m)" == 'x86_64' ]] || fail 'RPM spec targets x86_64 only'
need tar
need sed
VERSION="$(sed -n '/^\[workspace.package\]/,/^\[/s/^version[[:space:]]*=[[:space:]]*"\([0-9][0-9.]*\)".*/\1/p' Cargo.toml | head -n 1)"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail 'Could not read numeric workspace version'
tmp_dir="$(mktemp -d "$FPECS_ROOT/dist/.rpm-stage-XXXXXXXX")"
trap 'rm -rf -- "$tmp_dir"' EXIT
TOP="$tmp_dir/rpmbuild"
mkdir -p "$TOP"/{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS} "$tmp_dir/fpecs-package"
for bin in "${BINS[@]}"; do cp "$LINUX_OUT/$bin" "$tmp_dir/fpecs-package/$bin"; done
cp README.md LICENSE packaging/rpm/fpecs.desktop packaging/rpm/fpecs-db-setup "$tmp_dir/fpecs-package/"
cp packaging/assets/fpecs.svg "$tmp_dir/fpecs-package/fpecs.svg"
mkdir -p "$tmp_dir/fpecs-package/migrations"
cp migrations/001_initial_schema.sql migrations/002_runtime_snapshot.sql "$tmp_dir/fpecs-package/migrations/"
tar -czf "$TOP/SOURCES/fpecs-package.tar.gz" -C "$tmp_dir" fpecs-package
sed "s/@VERSION@/$VERSION/g" packaging/rpm/fpecs.spec.in > "$TOP/SPECS/fpecs.spec"
mkdir -p "$FPECS_ROOT/dist/rpm"
info "Building Fedora RPM (version $VERSION)"
rpmbuild -bb "$TOP/SPECS/fpecs.spec" --define "_topdir $TOP" --define "_rpmdir $FPECS_ROOT/dist/rpm"
info 'RPM generated under dist/rpm/x86_64/. System dependency/runtime testing still required.'
