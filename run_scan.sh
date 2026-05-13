#!/usr/bin/env sh
set -u

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

detect_target() {
    os=$(uname -s 2>/dev/null || echo unknown)
    arch=$(uname -m 2>/dev/null || echo unknown)

    case "$os:$arch" in
        Linux:x86_64|Linux:amd64)
            echo "x86_64-unknown-linux-musl"
            ;;
        Linux:aarch64|Linux:arm64)
            echo "aarch64-unknown-linux-musl"
            ;;
        Linux:armv7l|Linux:armv7*)
            echo "armv7-unknown-linux-musleabihf"
            ;;
        *)
            echo ""
            ;;
    esac
}

TARGET=$(detect_target)

if [ -n "$TARGET" ]; then
    BIN="$SCRIPT_DIR/dist/$TARGET/asic-discover"
    if [ -f "$BIN" ]; then
        chmod +x "$BIN" 2>/dev/null || true
        exec "$BIN" "$@"
    fi
fi

if [ -f "$SCRIPT_DIR/bin/asic-discover" ]; then
    chmod +x "$SCRIPT_DIR/bin/asic-discover" 2>/dev/null || true
    exec "$SCRIPT_DIR/bin/asic-discover" "$@"
fi

if [ -x "$SCRIPT_DIR/target/release/asic-discover" ]; then
    exec "$SCRIPT_DIR/target/release/asic-discover" "$@"
fi

if command -v cargo >/dev/null 2>&1; then
    cd "$SCRIPT_DIR" || exit 1
    cargo build --release || exit 1
    exec "$SCRIPT_DIR/target/release/asic-discover" "$@"
fi

EXPECTED_TARGET="${TARGET:-linux-architecture}"

cat >&2 <<EOF
asic-discover binary was not found for this Linux system.

Expected path:
  dist/$EXPECTED_TARGET/asic-discover

Fastest options:
  1. Copy a prebuilt Linux binary into that path.
  2. Install Rust/Cargo, then run:
     cd "$SCRIPT_DIR"
     cargo build --release
     ./target/release/asic-discover

On Debian/Ubuntu:
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
EOF

exit 1
