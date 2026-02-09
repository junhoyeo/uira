#!/usr/bin/env bash
set -euo pipefail

SESSION_NAME="uira-smoke-$$"
MODEL="anthropic/claude-sonnet-4-20250514"
TIMEOUT_SECONDS=90

cleanup() {
  tmux kill-session -t "$SESSION_NAME" >/dev/null 2>&1 || true
}
trap cleanup EXIT

if ! command -v tmux >/dev/null 2>&1; then
  echo "ERROR: tmux is required" >&2
  exit 1
fi

REPO_ROOT="$(git rev-parse --show-toplevel)"

echo "[1/6] Starting TUI in tmux session: $SESSION_NAME"
tmux new-session -d -s "$SESSION_NAME" -c "$REPO_ROOT" "cargo run -p uira-cli"

echo "[2/6] Waiting for TUI input prompt"
deadline=$((SECONDS + TIMEOUT_SECONDS))
while (( SECONDS < deadline )); do
  pane="$(tmux capture-pane -p -t "$SESSION_NAME" -S -80 || true)"
  if grep -q "Input (model:" <<<"$pane"; then
    break
  fi
  sleep 1
done

if ! grep -q "Input (model:" <<<"$(tmux capture-pane -p -t "$SESSION_NAME" -S -120 || true)"; then
  echo "ERROR: TUI did not reach input prompt within timeout" >&2
  exit 1
fi

echo "[3/6] Sending help and model switch commands"
tmux send-keys -t "$SESSION_NAME" "/help" Enter
sleep 1
tmux send-keys -t "$SESSION_NAME" "/model $MODEL" Enter
sleep 1
tmux send-keys -t "$SESSION_NAME" "/help" Enter
sleep 1

before_ctrl_l="$(tmux capture-pane -p -e -t "$SESSION_NAME" -S -4000)"
before_help_count="$(grep -o "Available commands:" <<<"$before_ctrl_l" | wc -l | tr -d ' ')"

echo "[4/6] Verifying Ctrl+L does not clear history"
tmux send-keys -t "$SESSION_NAME" C-l
sleep 1
after_ctrl_l="$(tmux capture-pane -p -e -t "$SESSION_NAME" -S -4000)"
after_help_count="$(grep -o "Available commands:" <<<"$after_ctrl_l" | wc -l | tr -d ' ')"

if (( after_help_count < before_help_count )); then
  echo "ERROR: Ctrl+L cleared chat history ($before_help_count -> $after_help_count)" >&2
  exit 1
fi

echo "[5/6] Verifying todo sidebar toggle behavior"
tmux send-keys -t "$SESSION_NAME" t
sleep 1
pane_after_hide="$(tmux capture-pane -p -e -t "$SESSION_NAME" -S -120)"
if ! grep -q "TODO sidebar hidden" <<<"$pane_after_hide"; then
  echo "ERROR: Missing 'TODO sidebar hidden' status after first toggle" >&2
  exit 1
fi

tmux send-keys -t "$SESSION_NAME" t
sleep 1
pane_after_show="$(tmux capture-pane -p -e -t "$SESSION_NAME" -S -120)"
if ! grep -q "TODO sidebar shown" <<<"$pane_after_show"; then
  echo "ERROR: Missing 'TODO sidebar shown' status after second toggle" >&2
  exit 1
fi

echo "[6/6] Verifying active model shown in input footer"
if ! grep -q "Input (model: $MODEL" <<<"$after_ctrl_l" && ! grep -q "Input (model: $MODEL" <<<"$pane_after_show"; then
  echo "ERROR: Active model not shown in input footer" >&2
  exit 1
fi

echo "PASS: tmux TUI smoke-check completed"
echo "- Help blocks before Ctrl+L: $before_help_count"
echo "- Help blocks after Ctrl+L:  $after_help_count"
echo "- Active model footer:        $MODEL"
