# Source before working on sudo (Git Bash):  source <repo>/dev.sh
# Points every tool cache this project uses at one data root, $SUDO_DATA:
#   - an existing $SUDO_DATA is kept;
#   - else F:\sudo-data, then D:\sudo-data, if present (the dev box: C: is nearly full);
#   - else <repo>/.data (gitignored), e.g. on a single-drive laptop.
_sudo_repo="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [ -z "${SUDO_DATA:-}" ]; then
  if [ -d /f/sudo-data/cargo ]; then SUDO_DATA='F:\sudo-data'
  elif [ -d /d/sudo-data ]; then SUDO_DATA='D:\sudo-data'
  else SUDO_DATA="$(cygpath -w "$_sudo_repo/.data")"; fi
fi
export SUDO_DATA
_sudo_data_u="$(cygpath -u "$SUDO_DATA")"
mkdir -p "$_sudo_data_u/uv-cache" "$_sudo_data_u/uv-python" "$_sudo_data_u/hf" "$_sudo_data_u/cuda-cache" "$_sudo_data_u/tmp"
export UV_CACHE_DIR="$SUDO_DATA\\uv-cache"
export UV_PYTHON_INSTALL_DIR="$SUDO_DATA\\uv-python"
export HF_HOME="$SUDO_DATA\\hf"
export CUDA_CACHE_PATH="$SUDO_DATA\\cuda-cache"
export CUDA_CACHE_MAXSIZE=4294967296
export RUSTUP_HOME="$SUDO_DATA\\rustup"
export CARGO_HOME="$SUDO_DATA\\cargo"
export npm_config_cache="$SUDO_DATA\\npm-cache"
export PLAYWRIGHT_BROWSERS_PATH="$SUDO_DATA\\ms-playwright"
export TEMP="$SUDO_DATA\\tmp" TMP="$SUDO_DATA\\tmp"
case ":$PATH:" in *":$_sudo_data_u/cargo/bin:"*) ;; *) export PATH="$_sudo_data_u/cargo/bin:$PATH";; esac
unset _sudo_repo _sudo_data_u
