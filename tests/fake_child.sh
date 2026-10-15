#!/usr/bin/env bash
trap 'echo finalized >> "$TARGET"; exit 0' INT
trap 'exit 143' TERM
: "${TARGET:?}"
printf 'fake-ready\n'
while true; do echo data >> "$TARGET"; sleep 0.2; done
