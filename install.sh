#!/usr/bin/env bash
set -euo pipefail

REPO="PewterZz/Beyond"
BINARIES="beyondtty beyonder"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

info()  { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
err()   { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

detect_platform() {
    local os arch
    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os" in
        Darwin) os="apple-darwin" ;;
        Linux)  os="unknown-linux-gnu" ;;
        *)      err "Unsupported OS: $os" ;;
    esac

    case "$arch" in
        x86_64|amd64)  arch="x86_64" ;;
        arm64|aarch64) arch="aarch64" ;;
        *)             err "Unsupported architecture: $arch" ;;
    esac

    echo "${arch}-${os}"
}

get_latest_version() {
    if command -v curl &>/dev/null; then
        curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
            | grep '"tag_name"' | head -1 | sed -E 's/.*"v?([^"]+)".*/\1/'
    elif command -v wget &>/dev/null; then
        wget -qO- "https://api.github.com/repos/${REPO}/releases/latest" \
            | grep '"tag_name"' | head -1 | sed -E 's/.*"v?([^"]+)".*/\1/'
    else
        err "curl or wget is required"
    fi
}

download() {
    local url="$1" dest="$2"
    if command -v curl &>/dev/null; then
        curl -fsSL -o "$dest" "$url"
    else
        wget -qO "$dest" "$url"
    fi
}

# Install NvChad — a batteries-included Neovim config — so nvim looks and
# behaves pleasantly the moment Beyond opens a file. Safe: we never clobber
# an existing ~/.config/nvim. Opt in with INSTALL_NVCHAD=1, or it auto-runs
# when no nvim config is present.
install_nvchad() {
    if ! command -v nvim &>/dev/null; then
        info "nvim not found on PATH — skipping NvChad setup."
        info "  Install nvim (brew install neovim / apt install neovim), then re-run with INSTALL_NVCHAD=1."
        return 0
    fi
    if ! command -v git &>/dev/null; then
        info "git not found — skipping NvChad setup."
        return 0
    fi

    local cfg="${XDG_CONFIG_HOME:-$HOME/.config}/nvim"
    if [ -e "$cfg" ]; then
        info "Existing nvim config at $cfg — leaving it alone."
        info "  To try NvChad, back up and reinstall:"
        info "    mv $cfg ${cfg}.bak && INSTALL_NVCHAD=1 $0"
        return 0
    fi

    info "Installing NvChad starter to $cfg..."
    git clone --depth 1 https://github.com/NvChad/starter "$cfg" >/dev/null 2>&1 \
        || { info "NvChad clone failed — skipping (you can rerun with INSTALL_NVCHAD=1)."; return 0; }
    rm -rf "$cfg/.git"
    info "NvChad installed. First 'nvim' launch will sync plugins (takes ~30s)."
}

main() {
    local version="${VERSION:-}"
    local platform
    platform="$(detect_platform)"

    if [ -z "$version" ]; then
        info "Fetching latest release..."
        version="$(get_latest_version)"
        [ -n "$version" ] || err "Could not determine latest version. Set VERSION=x.y.z manually."
    fi

    local tarball="beyondtty-v${version}-${platform}.tar.gz"
    local url="https://github.com/${REPO}/releases/download/v${version}/${tarball}"

    info "Downloading Beyond v${version} for ${platform}..."
    local tmpdir
    tmpdir="$(mktemp -d)"
    trap 'rm -rf "$tmpdir"' EXIT

    download "$url" "${tmpdir}/${tarball}"

    info "Extracting..."
    tar -xzf "${tmpdir}/${tarball}" -C "$tmpdir"

    for bin in $BINARIES; do
        if [ ! -f "${tmpdir}/${bin}" ]; then
            err "Binary '${bin}' not found in archive"
        fi
        info "Installing ${bin} to ${INSTALL_DIR}/${bin}..."
        if [ -w "$INSTALL_DIR" ]; then
            mv "${tmpdir}/${bin}" "${INSTALL_DIR}/${bin}"
        else
            sudo mv "${tmpdir}/${bin}" "${INSTALL_DIR}/${bin}"
        fi
        chmod +x "${INSTALL_DIR}/${bin}"
    done

    # NvChad: installs only when the user opts in (INSTALL_NVCHAD=1) AND
    # no nvim config is present. Never clobbers existing configs.
    if [ "${INSTALL_NVCHAD:-0}" = "1" ]; then
        install_nvchad
    fi

    info "Beyond v${version} installed to ${INSTALL_DIR}"
    echo ""
    echo "  Run 'beyondtty' or 'beyonder' to launch."
    echo ""
    echo "  Make sure you have a GPU that supports wgpu (Metal/Vulkan/DX12)"
    echo "  and at least one LLM provider running for agent features."
    echo ""
    echo "  Recommended:"
    echo "    - nvim (or vim / nano) — opened when you click a file link"
    echo "      in the block stream. Respects \$VISUAL / \$EDITOR."
    echo "    - NvChad — a modern Neovim config for a pleasant editor UI."
    echo "      Install it alongside Beyond with:"
    echo "        INSTALL_NVCHAD=1 curl -fsSL https://raw.githubusercontent.com/${REPO}/main/install.sh | bash"
    echo "      or manually: git clone --depth 1 https://github.com/NvChad/starter \\"
    echo "                   \"\${XDG_CONFIG_HOME:-\$HOME/.config}/nvim\""
}

main "$@"
