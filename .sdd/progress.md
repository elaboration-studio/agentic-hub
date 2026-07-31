# 0.16.0 SDD progress

- Task 1: complete (commits f64b146..ddf7df5, review clean) — Shared Manager table and Suite scope
  - Preserved source-qualified missing refs across same-ID source collisions.
  - Centralized Workspace → Suite entry so global data loads before Suite scope; cancelled workspace picks keep the current scope.
  - Reused validated capability row actions in Suite scope and restored aggregate/accessibility behavior.
  - Review verification: focused RED regressions confirmed, then `pnpm test` (21 files, 209 tests), `pnpm build`, and `git diff --check` passed; final review `Spec: PASS`, `Quality: APPROVED`.
- Task 2: implementation complete, awaiting review — Guided stale-copy recovery
  - Unowned stale projections stage a source refresh through the normal ActionBar Apply path.
  - Suite-owned stale projections re-sync the live selected suite, current base, and persisted/live manual extras without changing the binding.
  - Added typed missing-binding/missing-suite errors, preserved frontend `suiteId`, and replaced stale dots with accessible recovery menus plus validated source/target fallbacks.
  - Verification: `pnpm test` (22 files, 227 tests), `pnpm build`, focused core API tests (26), focused shell recovery tests (3), targeted Clippy for both libraries, and `git diff --check` passed.
- Task 3: pending — Today-first Statistics
- Task 4: pending — Resources order and ripgrep catalog
- Task 5: pending — Transactional direct palette shortcuts
- Task 6: pending — Release integration and verification
