# 0.16.0 SDD progress

- Task 1: complete (commits f64b146..ddf7df5, review clean) — Shared Manager table and Suite scope
  - Preserved source-qualified missing refs across same-ID source collisions.
  - Centralized Workspace → Suite entry so global data loads before Suite scope; cancelled workspace picks keep the current scope.
  - Reused validated capability row actions in Suite scope and restored aggregate/accessibility behavior.
  - Review verification: focused RED regressions confirmed, then `pnpm test` (21 files, 209 tests), `pnpm build`, and `git diff --check` passed; final review `Spec: PASS`, `Quality: APPROVED`.
- Task 2: pending — Guided stale-copy recovery
- Task 3: pending — Today-first Statistics
- Task 4: pending — Resources order and ripgrep catalog
- Task 5: pending — Transactional direct palette shortcuts
- Task 6: pending — Release integration and verification
