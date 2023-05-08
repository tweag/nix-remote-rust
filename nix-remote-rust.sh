#!/usr/bin/env bash

cd /home/jneeman/tweag/nix-remote-rust

# cargo build > /dev/null 2>&1

export RUST_BACKTRACE=1

./target/debug/nix-remote 2> /tmp/nix-remote-rust.log
