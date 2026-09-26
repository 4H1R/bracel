# Loaded by the canonical starter's windows.ps1 for Git Bash and child scripts.
# Import the launcher's complete Windows path, including Rust and Git tools.
if [ -n "${BRACEL_WINDOWS_PATH:-}" ]; then
    export PATH="$(/usr/bin/cygpath -p "$BRACEL_WINDOWS_PATH")"
    unset BRACEL_WINDOWS_PATH
fi
# Windows installs Python as python.exe; python3.exe may be a Store shortcut.
python3() { py.exe -3 "$@"; }
export -f python3
