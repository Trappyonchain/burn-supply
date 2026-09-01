#!/bin/sh
set -eu
cd "$(dirname "$0")"

case "${1:-}" in
  ""|--test-fixture) ;;
  *) printf '%s\n' 'Usage: sh contract/build.sh [--test-fixture]' >&2; exit 1 ;;
esac
if [ "$#" -gt 1 ]; then
  printf '%s\n' 'Usage: sh contract/build.sh [--test-fixture]' >&2
  exit 1
fi

# Empty directories prevent cargo-build-sbf from generating program keypairs.
# Refuse existing paths without opening any signing material.
mkdir -p target/deploy
for name in burned_fun mock_venue; do
  if [ -e "target/deploy/$name-keypair.json" ] || [ -L "target/deploy/$name-keypair.json" ]; then
    printf '%s\n' "Refusing to build: target/deploy/$name-keypair.json already exists." >&2
    exit 1
  fi
done
mkdir target/deploy/burned_fun-keypair.json target/deploy/mock_venue-keypair.json
cleanup() {
  rmdir target/deploy/burned_fun-keypair.json target/deploy/mock_venue-keypair.json 2>/dev/null || true
}
trap cleanup EXIT
# An interrupted compiler may still be running; retain its keypair guards.
trap 'trap - EXIT; printf "%s\n" "Build interrupted. Wait for compiler processes to exit before removing the empty keypair guard directories with rmdir." >&2; exit 1' HUP INT TERM

if [ "${1:-}" = --test-fixture ]; then
  cargo-build-sbf --lto --tools-version v1.52 --manifest-path programs/buyback/Cargo.toml --sbf-out-dir target/deploy -- --locked --features test-fixture
  cargo-build-sbf --lto --tools-version v1.52 --manifest-path tests/mock-venue/Cargo.toml --sbf-out-dir target/deploy -- --locked
else
  solana-verify build "$PWD" --url https://api.mainnet-beta.solana.com \
    --library-name burned_fun \
    --base-image solanafoundation/solana-verifiable-build@sha256:f71be5ca7620b7e40933b7f1294fa44e01d08c1fc5ba1f375a2478f5a01580d3 \
    --cargo-build-sbf-args='--lto --tools-version v1.52'
fi
test -d target/deploy/burned_fun-keypair.json
test -d target/deploy/mock_venue-keypair.json
test -f target/deploy/burned_fun.so
if [ "${1:-}" = --test-fixture ]; then test -f target/deploy/mock_venue.so; fi
