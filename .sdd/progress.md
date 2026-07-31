# 0.16.0 SDD progress

- Task 1: complete (commits f64b146..ddf7df5, review clean) — Shared Manager table and Suite scope
  - Preserved source-qualified missing refs across same-ID source collisions.
  - Centralized Workspace → Suite entry so global data loads before Suite scope; cancelled workspace picks keep the current scope.
  - Reused validated capability row actions in Suite scope and restored aggregate/accessibility behavior.
  - Review verification: focused RED regressions confirmed, then `pnpm test` (21 files, 209 tests), `pnpm build`, and `git diff --check` passed; final review `Spec: PASS`, `Quality: APPROVED`.
- Task 2: implementation complete, review findings fixed — Guided stale-copy recovery
  - Unowned stale projections stage a source refresh through the normal ActionBar Apply path.
  - Suite-owned stale projections re-sync the live selected suite, current base, and persisted/live manual extras without changing the binding.
  - Added typed missing-binding/missing-suite errors, preserved frontend `suiteId`, and replaced stale dots with accessible recovery menus plus validated source/target fallbacks.
  - Review hardening: `ApplySuiteResult` now reports projection, rule-sync, and hook-sync failures; recovery preserves prior bindings/manual extras on partial results; unrelated Manager staging survives re-sync refresh; all suite write paths share one process-wide projection transaction; warning control uses the semantic warning token and project Tooltip.
  - RED evidence: new core result-field tests initially failed to compile, the recovery failure regression dropped `skill:recorded`, and Manager regressions lost the staged key / emitted no partial-warning toast.
  - GREEN verification: `pnpm gen:types`; focused core recovery/error tests (2), shell recovery/transaction tests (5), and Manager/apply tests (37); `pnpm test` (22 files, 231 tests); `pnpm build`; core + shell tests; Task 2 Rust edits formatted (unrelated repo-wide formatter drift reverted); targeted all-feature Clippy; and `git diff --check`.
- Task 3: pending — Today-first Statistics
- Task 4: pending — Resources order and ripgrep catalog
- Task 5: pending — Transactional direct palette shortcuts
- Task 6: pending — Release integration and verification
