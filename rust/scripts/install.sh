#!/usr/bin/env bash
set -euo pipefail


BINARY_NAME="icode"
REPO_DIR="$(cd "$(dirname "$0")/.." && pwd)"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
BUILD_TARGET="${BUILD_TARGET:-}"
PROFILE="release"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

log()   { echo -e "${CYAN}▶${NC} $*"; }
ok()    { echo -e "${GREEN}✓${NC} $*"; }
warn()  { echo -e "${YELLOW}⚠${NC} $*"; }
err()   { echo -e "${RED}✗${NC} $*" >&2; }

usage() {
    cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Install ${BINARY_NAME} (icode CLI) globally.

Options:
  --prefix DIR       Install directory (default: ~/.local/bin)
  --target TARGET    Rust target triple (default: auto-detect)
  --debug            Build debug profile instead of release
  --uninstall        Remove the installed binary
  -h, --help         Show this help message

Environment:
  INSTALL_DIR        Override install directory
  BUILD_TARGET       Override Rust target triple

Examples:
  $(basename "$0")                      # Default install to ~/.local/bin
  $(basename "$0") --prefix /usr/local/bin   # System-wide install
  $(basename "$0") --debug              # Debug build
  $(basename "$0") --uninstall           # Remove binary
EOF
}

UNINSTALL=false
while [[ $# -gt 0 ]]; do
    case "$1" in
        --prefix)    INSTALL_DIR="$2"; shift 2 ;;
        --target)    BUILD_TARGET="$2"; shift 2 ;;
        --debug)     PROFILE="debug"; shift ;;
        --uninstall) UNINSTALL=true; shift ;;
        -h|--help)   usage; exit 0 ;;
        *)           err "Unknown option: $1"; usage; exit 1 ;;
    esac
done


if $UNINSTALL; then
    BIN_PATH="${INSTALL_DIR}/${BINARY_NAME}"
    if [[ -f "$BIN_PATH" ]]; then
        rm -v "$BIN_PATH"
        ok "Uninstalled ${BINARY_NAME} from ${BIN_PATH}"
    else
        warn "${BINARY_NAME} not found at ${BIN_PATH}"
    fi
    exit 0
fi

log "Checking prerequisites..."

if ! command -v cargo &>/dev/null; then
    err "cargo not found. Install Rust first: https://rustup.rs"
    exit 1
fi

RUST_VERSION="$(cargo --version)"
ok "Rust toolchain: ${RUST_VERSION}"

if [[ ! -f "${REPO_DIR}/Cargo.toml" ]]; then
    err "Cargo.toml not found at ${REPO_DIR}"
    exit 1
fi

log "Building ${BINARY_NAME} (${PROFILE})..."

cd "${REPO_DIR}"

CARGO_CMD=(cargo build -p icode-cli --profile "${PROFILE}")
if [[ -n "$BUILD_TARGET" ]]; then
    CARGO_CMD+=(--target "$BUILD_TARGET")
fi

"${CARGO_CMD[@]}"

if [[ -n "$BUILD_TARGET" ]]; then
    BIN_SOURCE="${REPO_DIR}/target/${BUILD_TARGET}/${PROFILE}/${BINARY_NAME}"
else
    BIN_SOURCE="${REPO_DIR}/target/${PROFILE}/${BINARY_NAME}"
fi

if [[ ! -f "$BIN_SOURCE" ]]; then
    err "Build completed but binary not found at ${BIN_SOURCE}"
    exit 1
fi

ok "Binary built: $(du -h "$BIN_SOURCE" | cut -f1)"

log "Installing to ${INSTALL_DIR}..."

mkdir -p "$INSTALL_DIR"
cp -f "$BIN_SOURCE" "${INSTALL_DIR}/${BINARY_NAME}"
chmod +x "${INSTALL_DIR}/${BINARY_NAME}"

ok "Installed: ${INSTALL_DIR}/${BINARY_NAME}"

if ! echo "$PATH" | tr ':' '\n' | grep -qxF "$INSTALL_DIR"; then
    warn "${INSTALL_DIR} is not in your PATH"
    echo ""
    echo "Add it to your shell config:"
    echo ""

    if [[ -f "$HOME/.bashrc" ]]; then
        echo "  # Bash: add to ~/.bashrc"
        echo "  echo 'export PATH=\"${INSTALL_DIR}:\$PATH\"' >> ~/.bashrc"
        echo "  source ~/.bashrc"
        echo ""
    fi

    if [[ -f "$HOME/.zshrc" ]]; then
        echo "  # Zsh: add to ~/.zshrc"
        echo "  echo 'export PATH=\"${INSTALL_DIR}:\$PATH\"' >> ~/.zshrc"
        echo "  source ~/.zshrc"
        echo ""
    fi

    echo "  # Or run directly:"
    echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
    echo ""

    export PATH="${INSTALL_DIR}:${PATH}"
fi

log "Verifying installation..."
if "${INSTALL_DIR}/${BINARY_NAME}" --version &>/dev/null; then
    VERSION="$("${INSTALL_DIR}/${BINARY_NAME}" --version 2>&1)"
    ok "${BINARY_NAME} ${VERSION}"
fi

echo ""
echo -e "${GREEN}╔══════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║  ${BINARY_NAME} installed successfully!                         ║${NC}"
echo -e "${GREEN}║  Run: ${NC}${BINARY_NAME} --help${GREEN}                            ║${NC}"
echo -e "${GREEN}║  Auth: ${NC}icode login${GREEN} or ${NC}ANTHROPIC_API_KEY=... icode${GREEN}   ║${NC}"
echo -e "${GREEN}╚══════════════════════════════════════════════════════╝${NC}"
echo ""
