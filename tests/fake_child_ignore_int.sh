#!/usr/bin/env bash
# Fake recorder that ignores SIGINT; only TERM (exit 143) or KILL stops it.
trap '' INT
trap 'exit 143' TERM
: "${TARGET:?}"
printf 'fake-ready\n'
while true; do echo data >> "$TARGET"; sleep 0.2; done
