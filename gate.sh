#!/usr/bin/env bash
# The one verification gate: format, lint, test, in that order, failing on the
# first. CI runs the same things as separate Nix checks; this is the single
# invocation for local and agent use.
#
# `web` is not a default feature, so it needs naming explicitly or src/web/ is
# never compiled and its tests never run. Features are additive, so the tests
# run once with `web` on rather than twice.
#
# The web clippy pass omits `--all-targets` deliberately: it lints src/web/,
# which nothing gated before, while skipping two pre-existing
# `await_holding_lock` hits in tests/integration/web_serve_test.rs. Drop the
# flag distinction once those are fixed.
#
# `lazyspec validate` is deliberately not here. The docs tree carries
# pre-existing findings from before the rules that catch them existed, so the
# gate would be red on arrival and tell you nothing about your change.
#
# `gate.sh fast` drops the default-feature clippy pass. That pass exists to
# catch code that only compiles with `web` on; everything else it would say,
# the `web` pass says too. Running one feature set means the test targets are
# compiled once instead of twice, which is most of the wall clock. Use it
# per-unit mid-batch; run the full gate before the batch is called done.
set -euo pipefail

cargo fmt --all -- --check

if [[ ${1:-} != fast ]]; then
	cargo clippy --all-targets --offline -- -D warnings
fi

# -A await_holding_lock: two pre-existing hits in tests/integration/web_serve_test.rs.
# Drop the flag once those are fixed.
cargo clippy --all-targets --features web --offline -- -D warnings -A clippy::await_holding_lock
cargo test --features web --offline
