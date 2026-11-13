#!/usr/bin/env bash
set -euo pipefail
# Piped from curl (`curl -fsSL …/install.sh | bash`): no checkout here, so fetch
# the source and run its installer from there.
if [[ -z ${BASH_SOURCE[0]:-} ]]; then
  command -v git >/dev/null || { echo "Git is needed to install nyx-calculator." >&2; exit 1; }
  work=$(mktemp -d)
  trap 'rm -rf "$work"' EXIT
  git clone -q --depth 1 https://github.com/design-nexus/nyx-calculator.git "$work/src"
  bash "$work/src/install.sh" "$@"
  exit
fi
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# A release archive (.prebuilt) brings its binaries.
if [[ ${NYX_SKIP_BUILD:-0} != 1 && ! -f $root/.prebuilt ]]; then
  cargo build --release --manifest-path "$root/Cargo.toml" -p nyx-calculator
fi
share="${XDG_DATA_HOME:-$HOME/.local/share}"
binary="${CARGO_TARGET_DIR:-$root/target}/release/nyx-calculator"
[[ -f $binary ]] || { echo "Release binary missing: $binary" >&2; exit 1; }
install -Dm755 "$binary" "$HOME/.local/bin/nyx-calculator"
install -Dm644 "$root/data/io.github.design_nexus.nyx.Calculator.desktop" "$share/applications/io.github.design_nexus.nyx.Calculator.desktop"
install -Dm644 "$root/data/icons/io.github.design_nexus.nyx.Calculator.svg" "$share/icons/hicolor/scalable/apps/io.github.design_nexus.nyx.Calculator.svg"
if command -v update-desktop-database >/dev/null; then update-desktop-database "$share/applications"; fi

# Hyprland: ensure Calculator opens as a floating window
hypr="${XDG_CONFIG_HOME:-$HOME/.config}/hypr"
if [[ ${NYX_SKIP_INTEGRATION:-0} != 1 && -d $hypr && -f $hypr/nyx.lua ]]; then
  if ! grep -qF 'nyx\\.Calculator' "$hypr/nyx.lua"; then
    printf 'o.window("^io\\\\.github\\\\.design_nexus\\\\.nyx\\\\.Calculator$", { float = true, center = true })\n' >> "$hypr/nyx.lua"
    if command -v hyprctl >/dev/null 2>&1; then
      hyprctl reload >/dev/null 2>&1 || true
    fi
  fi
fi

printf 'Installed Calculator without opening it.\n'
