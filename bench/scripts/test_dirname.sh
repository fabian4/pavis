#!/bin/bash
echo "BASH_SOURCE[0]=${BASH_SOURCE[0]}"
echo "dirname result=$(dirname "${BASH_SOURCE[0]}")"
DIR_PART="$(dirname "${BASH_SOURCE[0]}")"
echo "About to cd to: $DIR_PART"
cd "$DIR_PART" && echo "After cd, pwd=$(pwd)"
