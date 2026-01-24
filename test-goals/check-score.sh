#!/bin/bash
# Simple goal command that outputs a score
# Usage: ./check-score.sh [score]
# Defaults to 85.5 if no arg

SCORE=${1:-85.5}
echo "Running validation..."
echo "Checking files..."
sleep 0.1
echo "$SCORE"
