#!/usr/bin/env bash
# The one verification gate: format, lint, test, in that order, failing on the
# first. CI runs the same three things as separate Nix checks; this is the
# single invocation for local and agent use.
#
# `lazyspec validate` is deliberately not here. The docs tree carries
# pre-existing findings from before the rules that catch them existed, so the
# gate would be red on arrival and tell you nothing about your change.
set -euo pipefail

cargo fmt --all -- --check
cargo clippy --all-targets --offline -- -D warnings
cargo test --offline
