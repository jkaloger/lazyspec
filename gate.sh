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
set -euo pipefail

cargo fmt --all -- --check
cargo clippy --all-targets --offline -- -D warnings
cargo clippy --features web --offline -- -D warnings
cargo test --features web --offline
