#!/usr/bin/env bash
set -o errexit
set -o nounset
set -o pipefail

trap "exit" INT TERM
trap "sudo kill 0" EXIT

SCRIPT_ROOT=$(realpath $(dirname "${BASH_SOURCE}"))

sudo "${SCRIPT_ROOT}/profile.py" $@