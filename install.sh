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

# Linux: register Beyondtty with the freedesktop launcher (drun, rofi, GNOME
# Activities, KDE Krunner, etc.) by writing a .desktop entry and icon. Opt out
# with SKIP_DESKTOP=1.
install_desktop_entry() {
    [ "${SKIP_DESKTOP:-0}" = "1" ] && return 0

    local apps_dir icons_dir
    if [ "$(id -u)" = "0" ] && { [ "$INSTALL_DIR" = "/usr/local/bin" ] || [ "$INSTALL_DIR" = "/usr/bin" ]; }; then
        apps_dir="/usr/share/applications"
        icons_dir="/usr/share/icons/hicolor/256x256/apps"
    else
        apps_dir="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
        icons_dir="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/256x256/apps"
    fi
    mkdir -p "$apps_dir" "$icons_dir"

    local icon_url="https://raw.githubusercontent.com/${REPO}/v${1}/assets/beyond-256.png"
    local icon_path="${icons_dir}/beyondtty.png"
    if download "$icon_url" "$icon_path" 2>/dev/null; then
        info "Installed icon to ${icon_path}"
    else
        # Try main branch as a fallback (older tags may not have the asset).
        download "https://raw.githubusercontent.com/${REPO}/main/assets/beyond-256.png" "$icon_path" 2>/dev/null \
            || info "Couldn't fetch icon — .desktop entry will use a generic icon."
    fi

    local desktop_path="${apps_dir}/beyondtty.desktop"
    cat > "$desktop_path" <<EOF
[Desktop Entry]
Type=Application
Name=Beyondtty
GenericName=AI-Native Terminal
Comment=Block-oriented terminal with first-class AI agents
Exec=beyondtty
Icon=beyondtty
Terminal=false
Categories=Utility;TerminalEmulator;Development;
Keywords=terminal;shell;ai;agent;llm;
StartupWMClass=beyondtty
EOF
    info "Installed desktop entry to ${desktop_path}"

    command -v update-desktop-database >/dev/null && update-desktop-database "$apps_dir" 2>/dev/null || true
    command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t -f "$(dirname "$(dirname "$(dirname "$icons_dir")")")" 2>/dev/null || true
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

    # Linux: register a .desktop entry so launchers (drun/rofi/GNOME/KDE) find it.
    case "$platform" in
        *linux*) install_desktop_entry "$version" ;;
    esac

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
