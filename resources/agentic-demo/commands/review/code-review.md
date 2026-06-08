---
description: Review the staged diff for correctness, clarity, and risk before it lands.
---

# Code review

Review the current change as a careful senior engineer.

1. Read the diff in full before commenting — understand intent, not just lines.
2. Flag correctness bugs, race conditions, and unhandled errors first.
3. Note clarity issues: naming, dead code, comments that narrate instead of explain.
4. Check tests: does new behavior have a test, and does each test assert one thing?
5. Summarize with a short verdict — ship, ship-with-nits, or needs-work — and the top 3 actions.

Keep feedback specific and kind. Quote the exact line when you raise an issue.
