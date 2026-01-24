#!/bin/bash
# Goal: Check if progress.txt contains a number >= target
# Outputs the number from progress.txt (or 0 if missing/invalid)

PROGRESS_FILE="./progress.txt"

if [ ! -f "$PROGRESS_FILE" ]; then
    echo "0"
    exit 0
fi

# Read the last line that contains a number
SCORE=$(grep -E '^[0-9]+\.?[0-9]*$' "$PROGRESS_FILE" | tail -1)

if [ -z "$SCORE" ]; then
    echo "0"
else
    echo "$SCORE"
fi
