#!/usr/bin/env bash
# build.sh — devmail build script (Git Bash on Windows, or bash on Linux/macOS)
set -euo pipefail

VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*= *"\(.*\)"/\1/')

cmd="${1:-help}"
shift || true   # consume the subcommand; remaining $@ are passed through if needed

case "$cmd" in

  # ── Windows native build ────────────────────────────────────────────────────

  build-windows)
    cargo build --release
    echo "Binary: target/release/devmail.exe"
    ;;

  release-windows)
    cargo build --release
    mkdir -p dist
    WIN_NAME="devmail-v${VERSION}-windows-x86_64"
    powershell -NoProfile -Command "
      New-Item -ItemType Directory -Force -Path 'dist/${WIN_NAME}' | Out-Null
      Copy-Item 'target/release/devmail.exe','LICENSE.md','README.md' -Destination 'dist/${WIN_NAME}'
      Compress-Archive -Force -Path 'dist/${WIN_NAME}' -DestinationPath 'dist/${WIN_NAME}.zip'
      Remove-Item -Recurse -Force 'dist/${WIN_NAME}'
    "
    echo "Windows release: dist/${WIN_NAME}.zip"
    ;;

  # ── Linux binary via Docker Desktop ────────────────────────────────────────

  build-linux)
    mkdir -p dist
    DOCKER_BUILDKIT=1 docker build \
      -f Dockerfile.linux-build \
      --output type=local,dest=./dist \
      .
    echo "Linux binary: dist/devmail"
    ;;

  release-linux)
    bash "$0" build-linux
    LINUX_NAME="devmail-v${VERSION}-linux-x86_64"
    mkdir -p "dist/${LINUX_NAME}"
    cp dist/devmail LICENSE.md README.md "dist/${LINUX_NAME}/"
    tar -czf "dist/${LINUX_NAME}.tar.gz" -C dist "${LINUX_NAME}"
    rm -rf "dist/${LINUX_NAME}"
    echo "Linux release: dist/${LINUX_NAME}.tar.gz"
    ;;

  # ── Run locally ─────────────────────────────────────────────────────────────

  run)
    cargo run --release
    ;;

  run-store)
    cargo run --release -- --store
    ;;

  # ── Tests ───────────────────────────────────────────────────────────────────

  test)
    cargo test -- --test-threads=1
    ;;

  # ── Docker test container ───────────────────────────────────────────────────

  test-container)
    bash "$0" build-linux
    docker build -f Dockerfile.test -t devmail-test .
    docker run --rm -p 1025:1025 -p 8085:8085 devmail-test
    ;;

  # ── Release both platforms ──────────────────────────────────────────────────

  all)
    bash "$0" release-windows
    bash "$0" release-linux
    ;;

  # ── Clean ───────────────────────────────────────────────────────────────────

  clean)
    cargo clean
    rm -rf dist
    echo "Cleaned."
    ;;

  # ── Help ────────────────────────────────────────────────────────────────────

  help|--help|-h|*)
    echo "Usage: ./build.sh <command>"
    echo ""
    echo "  build-windows    Build Windows release binary (cargo build --release)"
    echo "  release-windows  build-windows + package dist/devmail-v${VERSION}-windows-x86_64.zip"
    echo "  build-linux      Build Linux x86_64 binary via Docker Desktop"
    echo "  release-linux    build-linux + package dist/devmail-v${VERSION}-linux-x86_64.tar.gz"
    echo "  all              release-windows + release-linux"
    echo ""
    echo "  run              cargo run --release"
    echo "  run-store        cargo run --release -- --store"
    echo "  test             cargo test"
    echo ""
    echo "  test-container   build-linux, build Docker test image, run it"
    echo "  clean            cargo clean + rm dist/"
    ;;

esac
