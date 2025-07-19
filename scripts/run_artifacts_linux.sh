#!/bin/sh

trap 'kill $(jobs -p)' EXIT

./kodama-login &
./kodama-patch &
./kodama-web &
./kodama-lobby &
./kodama-world &
wait
