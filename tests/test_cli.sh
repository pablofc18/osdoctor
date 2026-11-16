#!/bin/sh
# test_cli.sh - CLI integration tests against the built binary: exit codes,
# the exact version line, the usage text, and well-formed --json output.
# Mirrors tests/cli.rs and the cli unit tests of the Rust implementation.
#
# Usage: tests/test_cli.sh ./osdoctor

BIN=${1:-./osdoctor}
checks=0
failures=0

fail() {
    failures=$((failures + 1))
    echo "  FAIL: $1"
}

# expect_exit <code> <args...>
expect_exit() {
    want=$1
    shift
    checks=$((checks + 1))
    "$BIN" "$@" >/dev/null 2>&1
    got=$?
    [ "$got" -eq "$want" ] || fail "'osdoctor $*' exited $got, want $want"
}

echo "test_cli"

version=$(sed -n 's/^#define OSDOCTOR_VERSION "\(.*\)"$/\1/p' include/osdoctor.h)
checks=$((checks + 1))
[ "$("$BIN" version)" = "osdoctor $version" ] || fail "version line"

expect_exit 0 version
expect_exit 0 help
expect_exit 0 -V
expect_exit 0 --help
expect_exit 3              # no command
expect_exit 3 bogus        # unknown command
expect_exit 3 --nope       # unknown option
expect_exit 3 scan system  # extra argument

help=$("$BIN" help)
checks=$((checks + 1))
case $help in
"osdoctor $version - Linux desktop health checks"*) ;;
*) fail "usage starts with name and version" ;;
esac
for needle in "Usage:" "  osdoctor <command> [options]" "Exit codes:"; do
    checks=$((checks + 1))
    printf '%s\n' "$help" | grep -qxF -- "$needle" || fail "usage contains '$needle'"
done

# scan --json: exit code depends on the host, but must be 0/1/2 and the
# output must be well-formed JSON.
checks=$((checks + 1))
json=$("$BIN" scan --json)
code=$?
case $code in
0 | 1 | 2) ;;
*) fail "scan --json exited $code" ;;
esac
checks=$((checks + 1))
if command -v python3 >/dev/null 2>&1; then
    printf '%s\n' "$json" | python3 -m json.tool >/dev/null || fail "scan --json is not valid JSON"
else
    case $json in
    "{"*"\"groups\""*"}") ;;
    *) fail "scan --json is not JSON-shaped" ;;
    esac
fi

echo "  $checks checks, $failures failures"
[ "$failures" -eq 0 ]
