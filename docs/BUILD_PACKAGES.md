# Building FPECS Linux packages on Fedora

The source is the Rust workspace, no prebuilt binary or personal data is required.
These scripts produce binary packages locally and should be committed as source, not the generated `.rpm` / `.deb` files.

**Recommended:** Generate and commit `Cargo.lock` for reproducible releases (`cargo generate-lockfile` if it is missing). A build without `Cargo.lock` is supported but will re-resolve dependencies.

## RPM (Fedora x86_64)

```bash
sudo dnf install rpm-build
./scripts/04_build_release.sh
```

Output: `dist/rpm/x86_64/*.rpm`.
For a packaging-only retry after native Rust compilation:
`./scripts/04_build_release.sh --rpm-only`.

## DEB (Ubuntu 24.04 amd64, built locally on Fedora)

```bash
sudo dnf install podman
./scripts/09_build_deb_fedora.sh
```

Podman downloads an Ubuntu image, builds Rust **within Ubuntu**, and packages the native binaries. This requires an internet connection and considerable disk space on the first build.
Output: `dist/deb/fpecs_<version>_amd64.deb`.

On Ubuntu/Debian hosts with dependencies and Rust installed, `bash packaging/deb/build-deb.sh` builds a native DEB without Podman.

## What to commit to GitHub

Commit **only** the packaging tools and updated RPM specification, alongside the existing source:

```bash
git add scripts/04_build_release.sh scripts/_common.sh scripts/09_build_deb_fedora.sh \
  packaging/deb/build-deb.sh packaging/rpm/fpecs.spec.in docs/BUILD_PACKAGES.md
git diff --cached --name-only
git commit -m "Add local RPM and Ubuntu DEB build scripts"
git push
```

No need to commit any other scripts. Output binaries remain ignored under `dist/`.
Only publish tested RPM/DEB binaries to GitHub **Releases**. Debian compatibility beyond Ubuntu 24.04 is not guaranteed; test on target systems.

## Test installation

On **Fedora**: `sudo dnf install ./dist/rpm/x86_64/fpecs-*.rpm`.
On **Ubuntu 24.04**: `sudo apt install ./fpecs_1.0.0_amd64.deb` (use your actual filename).

PostgreSQL is optional, and the packages do not install/create a local PostgreSQL server. The app can run using local storage or connect to a database in **Settings → Storage & Connection**.
