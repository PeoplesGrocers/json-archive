#!/usr/bin/env bash

case $1 in
    copyright)
        addlicense -c "Peoples Grocers LLC" -f LICENSE-header -l "agpl-3.0" -s src/ >&2
    ;;


esac
