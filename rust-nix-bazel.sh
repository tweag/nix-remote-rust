#!/usr/bin/env bash

cd /home/jneeman/tweag/rust-nix-bazel

# cargo build > /dev/null 2>&1

export RUST_BACKTRACE=1

./target/debug/rust-nix-bazel 2> /tmp/rust-nix-bazel.log
