# Phase Breakdown Rules

## Structure

Every feature's implementation plan is broken into **phase files** under `plans/<feature>/`:

```
plans/<feature>/
├── README.md          # Design doc (goal, manifesto alignment, non-goals, unknowns, API surface, type flow, E2E acceptance)
├── phase-01-<slug>.md
├── phase-02-<slug>.md
└── ...
```

The design doc (`README.md`) contains the high-level design. Phases are separate files — one per vertical slice.

## Phase File Format

```markdown
# Phase N: <Phase Name>

## Goal

One sentence: what this phase delivers end-to-end.

## Tasks

### Task 1: <Short description>

**Files** (max 5):
- `path/to/file1.rs` (new / modified)
- `path/to/file2.rs` (modified)

**Changes**:
- Bullet list of what to do

**Acceptance Criteria**:
```rust
#[test]
fn descriptive_test_name() {
    // Given: ...
    // When: ...
    // Then: ...
}
```

### Task 2: <Short description>

**Files** (max 5):
...

## Phase Acceptance Criteria

Integration-level criteria that validate the phase works as a whole.

## Dependencies

Which phases must be complete before this one starts.
```

## Rules

1. **Max 5 files per task** — if a task touches more than 5 files, split it. This keeps changes reviewable and reduces merge conflict risk.
2. **Every task has acceptance criteria** — concrete tests (Given/When/Then). No task is done without a passing test.
3. **Strict TDD per task** — implement each task following Red → Green → Refactor (see `tdd.md`). Write the failing test from the acceptance criteria FIRST, then the minimal code to make it pass, then refactor. Never write implementation before the test exists and fails.
4. **Phases are self-contained** — each phase file has everything an agent needs to implement it without reading other phases. Reference the design doc for context, not sibling phases.
5. **Tasks are ordered** — within a phase, tasks are listed in implementation order. Earlier tasks may be dependencies of later ones.
6. **One phase = one vertical slice** — each phase delivers something usable end-to-end, not "internals first, integrate later."
7. **Design doc stays high-level** — the `README.md` has the architecture, API surface, type flow, and E2E tests. Implementation details live in phase files.

## Migrating Existing Plans

When a plan exists as a single markdown file (`plans/<feature>.md`):
1. Create `plans/<feature>/` directory
2. Move the design sections into `plans/<feature>/README.md`
3. Break implementation phases into individual `phase-NN-<slug>.md` files
4. Delete the original single file
