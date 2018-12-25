#!/usr/bin/env bash

set -e

cargo build --release
./halite --replay-directory replays/ -vvv --width 32 --height 32 "RUST_BACKTRACE=1 ./target/release/my_bot" "RUST_BACKTRACE=1 ./target/release/my_bot"
