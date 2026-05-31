#!/usr/bin/env bash
# Demo PostToolUse hook for Agentic Hub.
# Replace this body with your real formatter / linter / audit logic.
# Available env vars (per tool):
#   Cursor : payload via stdin (see https://cursor.com/docs/hooks.md)
#   Claude : $CLAUDE_TOOL_NAME, $CLAUDE_TOOL_INPUT, $CLAUDE_TOOL_OUTPUT, $CLAUDE_PROJECT_DIR
#   Codex  : payload via stdin (see https://developers.openai.com/codex/hooks)
set -euo pipefail
echo "[agentic-hub] auto-format-after-edit fired" >&2
exit 0
