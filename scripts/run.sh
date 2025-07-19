#!/bin/sh

trap 'kill $(jobs -p)' EXIT

cargo run -q --package kodama --bin kodama-login &
cargo run -q --package kodama --bin kodama-patch &
cargo run -q --package kodama --bin kodama-web &
cargo run -q --package kodama --bin kodama-lobby &
cargo run -q --package kodama --bin kodama-world &
wait
