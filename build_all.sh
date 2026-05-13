#!/usr/bin/env bash
set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DIST_DIR="$SCRIPT_DIR/dist"
INSTALL_TARGETS=0
KEEP_GOING=0
TARGETS=()

DEFAULT_TARGETS=(
  "x86_64-unknown-linux-musl"
  "aarch64-unknown-linux-musl"
  "armv7-unknown-linux-musleabihf"
  "x86_64-pc-windows-gnu"
)

usage() {
  cat <<'EOF'
Usage:
  ./build_all.sh [--install-targets] [--keep-going] [--target TARGET ...]

Examples:
  ./build_all.sh --target x86_64-unknown-linux-musl
  ./build_all.sh --install-targets --keep-going
  ./build_all.sh --target x86_64-pc-windows-gnu
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --install-targets)
      INSTALL_TARGETS=1
      shift
      ;;
    --keep-going)
      KEEP_GOING=1
      shift
      ;;
    --target)
      if [[ $# -lt 2 ]]; then
        echo "--target requires a value" >&2
        exit 2
      fi
      TARGETS+=("$2")
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ ${#TARGETS[@]} -eq 0 ]]; then
  TARGETS=("${DEFAULT_TARGETS[@]}")
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo was not found" >&2
  exit 1
fi

if [[ "$INSTALL_TARGETS" -eq 1 ]] && ! command -v rustup >/dev/null 2>&1; then
  echo "rustup was not found; cannot install Rust targets automatically" >&2
  exit 1
fi

mkdir -p "$DIST_DIR"
SHA_FILE="$DIST_DIR/SHA256SUMS.txt"
rm -f "$SHA_FILE"
FAILURES=()

cd "$SCRIPT_DIR" || exit 1

for TARGET in "${TARGETS[@]}"; do
  echo "==> target: $TARGET"

  if [[ "$INSTALL_TARGETS" -eq 1 ]]; then
    if ! rustup target add "$TARGET"; then
      FAILURES+=("$TARGET target install failed")
      [[ "$KEEP_GOING" -eq 1 ]] && continue || break
    fi
  fi

  if ! cargo build --release --target "$TARGET"; then
    echo "warning: build failed for $TARGET" >&2
    echo "hint: rustup target add $TARGET" >&2
    echo "hint: linux-gnu and windows-gnu targets can require external cross-linkers; prefer musl targets for portable Linux binaries." >&2
    FAILURES+=("$TARGET build failed")
    [[ "$KEEP_GOING" -eq 1 ]] && continue || break
  fi

  EXE_NAME="asic-discover"
  if [[ "$TARGET" == *windows* ]]; then
    EXE_NAME="asic-discover.exe"
  fi

  BUILT="$SCRIPT_DIR/target/$TARGET/release/$EXE_NAME"
  if [[ ! -f "$BUILT" ]]; then
    FAILURES+=("$TARGET binary not found at $BUILT")
    [[ "$KEEP_GOING" -eq 1 ]] && continue || break
  fi

  TARGET_DIST="$DIST_DIR/$TARGET"
  mkdir -p "$TARGET_DIST"
  cp "$BUILT" "$TARGET_DIST/$EXE_NAME"
  cp "$SCRIPT_DIR/README.md" "$TARGET_DIST/README.md"

  if command -v sha256sum >/dev/null 2>&1; then
    (cd "$DIST_DIR" && sha256sum "$TARGET/$EXE_NAME") >> "$SHA_FILE"
  elif command -v shasum >/dev/null 2>&1; then
    (cd "$DIST_DIR" && shasum -a 256 "$TARGET/$EXE_NAME") >> "$SHA_FILE"
  fi

  echo "    built: $TARGET_DIST/$EXE_NAME"
done

if [[ ${#FAILURES[@]} -gt 0 ]]; then
  echo "Some targets failed:" >&2
  printf '  %s\n' "${FAILURES[@]}" >&2
  exit 1
fi

echo "Done. Artifacts are in: $DIST_DIR"
