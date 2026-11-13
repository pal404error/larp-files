#!/bin/sh
# Offline test for install.sh checksum verification (LARP-06).
# Runs the real installer against a local file:// "release" and checks that
# a matching binary installs, and that a tampered binary or a release with
# no SHA256SUMS installs nothing.
# Usage: sh tests/install_checksum.sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT INT TERM

# Pretend to be Linux so the test also runs on macOS and Git Bash.
mkdir -p "$WORK/bin" "$WORK/rel/latest/download"
printf '#!/bin/sh\necho Linux\n' > "$WORK/bin/uname"
chmod +x "$WORK/bin/uname"

REL="$WORK/rel/latest/download"
if command -v cygpath >/dev/null 2>&1; then
  BASE="file:///$(cygpath -m "$WORK/rel")"
else
  BASE="file://$WORK/rel"
fi
if command -v sha256sum >/dev/null 2>&1; then
  mksums() { (cd "$REL" && sha256sum larp-Linux > SHA256SUMS); }
else
  mksums() { (cd "$REL" && shasum -a 256 larp-Linux > SHA256SUMS); }
fi

run_install() {
  PATH="$WORK/bin:$PATH" INSTALL_DIR="$WORK/out" LARP_DOWNLOAD_BASE="$BASE" \
    sh "$ROOT/install.sh" >"$WORK/log" 2>&1
}
fail() { echo "FAIL: $1" >&2; cat "$WORK/log" >&2; exit 1; }

# 1. Matching checksum: installed byte-for-byte and executable.
# A shebang so Git Bash (Windows) also reports the file as executable.
printf '#!/bin/sh\necho genuine larp binary\n' > "$REL/larp-Linux"
mksums
run_install || fail "genuine binary was refused"
cmp -s "$WORK/out/larp" "$REL/larp-Linux" || fail "installed file differs"
[ -x "$WORK/out/larp" ] || fail "installed file is not executable"

# 2. Tampered binary: refused, previous install untouched.
printf 'evil binary\n' > "$REL/larp-Linux"
if run_install; then fail "tampered binary was installed"; fi
grep -q "Checksum mismatch" "$WORK/log" || fail "no mismatch message"
grep -q "genuine" "$WORK/out/larp" || fail "previous install was overwritten"

# 3. Release without SHA256SUMS: refused.
rm -rf "$WORK/out"
rm -f "$REL/SHA256SUMS"
if run_install; then fail "unverifiable binary was installed"; fi
[ ! -e "$WORK/out/larp" ] || fail "file installed without verification"

echo "install.sh checksum tests: ok"
