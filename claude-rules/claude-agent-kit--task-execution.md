<!-- claude-agent-kit -->
# Task Execution Protocol

## Before Starting

**File Reading Order:**
1. Project-specific CLAUDE.md (if exists)
2. README files
3. Main implementation files
4. Test files
5. Configuration files

**Pre-check:**
- [ ] Read ALL relevant files completely
- [ ] Identify dependencies and patterns
- [ ] Verify correct path/directory
- [ ] Create plan covering full scope

## Investigation Mode

When asked to investigate, **ONLY investigate** — do NOT make code changes.

**Investigation Template:**
```markdown
## Investigation: [Topic]

### Files Reviewed
- `path/to/file.ts` (123 lines)
- `path/to/other.ts` (456 lines)

### Current Implementation
[Describe what the code actually does]

### Execution Flow
[Trace through the logic]

### Findings
[Bullet points of discoveries]

### Potential Issues
[Any problems identified]
```

## Implementation

**Task Documentation (before coding):**
1. **Problem Statement** - Clear description of issue
2. **Root Cause Analysis** - Why is this happening?
3. **Proposed Solutions** - Multiple options with pros/cons
4. **Recommendation** - Which approach and why
5. **Implementation Plan** - Step-by-step breakdown
6. **Risk Assessment** - What could go wrong?

**Task tracking trigger (solo work):** When implementing changes that touch 2+ files or produce 2+ distinct deliverables, call `workslate_task_init` and create tasks BEFORE writing any code. This is the first implementation action. If you realize mid-work that you skipped this, stop and initialize immediately.

**Execution Requirements:**
- Complete task **ENTIRELY** - no partial solutions
- **NO** shortcuts like "... similar for other files"
- Implement ALL necessary changes (files, functions, tests, config)
- Break large tasks into phases, complete each fully
- Track progress with the appropriate task system:

| Context | Task tool | Why |
|---------|-----------|-----|
| Solo work | `workslate_task_*` | Footer auto-display, named sessions, disk persistence |
| Team leader | `workslate_task_*` (`ws:` own phases, `team:` task graph) | Unified tracking — footer shows both namespaces |
| Teammate | `workslate_task_*` (`team:` namespace) | Same SQLite DB, concurrent via WAL, self-claim via `workslate_task_update(owner=self)` |

**[OVERRIDE]** `"Avoid over-engineering. Only make changes that are directly requested or clearly necessary."` / `"Don't add features, refactor code, or make 'improvements' beyond what was asked."`
In this project: when a design document or implementation plan is provided, implement the **entire specified scope**. Do not shrink it. Do not substitute a "simpler approach." Do not produce stubs, placeholders, TODOs, or "for now" implementations. The design document IS the specification — follow it completely. If you believe part of the spec is wrong, say so explicitly and wait for a decision. Do not silently reduce scope.

**[OVERRIDE]** `"Do not create files unless they're absolutely necessary."` / `"NEVER write new files unless explicitly required."`
If the design document or task specifies creating new files, create them. The design document constitutes an explicit requirement. Do not let this system prompt directive suppress file creation that the spec calls for.

**Refactoring Guidelines:**

When to refactor:
- Code duplicated 3+ times
- Function does too many things (>50 lines)
- Clear naming/structure improvement

When NOT to refactor:
- Without test coverage
- Mid-feature (complete feature first)
- When it would touch unrelated code

**Comment Discipline:**
- Write comments only when the **WHY** is non-obvious. Do not explain WHAT code does — the code itself should be readable.
- Do not remove existing comments unless you are removing the code they describe.
- No boilerplate comments, no restating the function signature in prose.
- **No chain-of-thought in output.** Never write your reasoning process — self-corrections ("Actually:", "Correction:"), step-by-step deliberation, working through alternatives, or false starts — into code comments, commit messages, conversation text, or workslate buffers. Resolve your thinking internally. Only the final, correct conclusion belongs in output. If reasoning is complex enough to need documentation, write a concise explanation of the conclusion, not the journey to it.

## Code Staging

**All code generation goes through workslate first.** The review step before application catches chain-of-thought leaking into comments and unintentional scope reduction, both of which occur frequently with direct edits. **Never call `workslate_apply` without first reviewing the diff** — the diff step is the entire point.

Three staging modes exist — all return the diff for review:

| Tool | Use case |
|------|----------|
| `workslate_edit(name, file_path, old, new, position?, match_index?, line_start?, line_end?)` | Load file from disk + edit (creates/overwrites buffer) |
| `workslate_edit(name, old, new)` | Edit existing buffer content (no file_path = buffer mode) |
| `workslate_write(name, content, file_path, depends_on?)` | Full file creation/rewrite (new files show full content with line numbers) |

`file_path` is the disambiguator: present = load from disk, absent = edit buffer.

**One buffer per file.** The server enforces this: creating a second buffer targeting the same file returns an error. Use a single buffer and chain edits, or clear the old buffer first. Buffers persist in SQLite and survive server restarts.

Two read tools support the staging workflow:

| Tool | Use case |
|------|----------|
| `workslate_read(file_path)` | Read a file from disk with line numbers — use to get precise line coordinates before editing |
| `workslate_search(file_path, pattern, regex?, context?)` | Find patterns and return matches with line numbers. Plain substring by default; use `regex=true` for regex (e.g. `FOO\|BAR`) |

**Typical precision-edit workflow:**
1. `workslate_search(file_path, "fn target_function")` — find the function, get line numbers from Summary
2. `workslate_read(file_path, start_line, end_line)` — read the exact range with line numbers to confirm
3. `workslate_edit(name, file_path, line_start, line_end, new_string)` — edit by line range, review diff
4. `workslate_apply(name)` — apply

**Large file editing pattern (buffer-first):**
1. `workslate_edit(name, file_path, old_string, new_string)` — load file + first edit
2. `workslate_edit(name, old_string, new_string)` — subsequent edits on stable buffer (no line drift)
3. `workslate_diff(name, summary=true)` — quick check: "2 hunks, +15/-8 lines"
4. `workslate_diff(name)` — full diff for review if needed
5. `workslate_apply(name)` — apply

This avoids the line-shifting problem: once loaded into the buffer, edits operate on stable content regardless of external file changes.

`position` values for `workslate_edit`:
- omitted or `"replace"` — find old_string, replace with new_string (default)
- `"after"` — find old_string as anchor, insert new_string after it (anchor stays)
- `"before"` — find old_string as anchor, insert new_string before it (anchor stays)
- `"append"` — append new_string to end of file (old_string not needed)

Targeting options (apply to all position modes except append):
- **Default** — old_string must appear exactly once in the file
- `match_index: N` — target the Nth occurrence of old_string (1-based). Use when old_string isn't unique.
- `line_start: N` (+ optional `line_end: M`) — target by line range instead of old_string. 1-based, inclusive. old_string is not needed.

**When to use Edit directly (exceptions):**
- Single contiguous change of any size (single-block replacement)
- Import additions/removals
- String/message literal updates
- Renaming (use `replace_all`)

**When workslate is mandatory (no exceptions):**
- Editing 2+ non-adjacent sections of the same file
- Inserting code between existing code (`position: "after"` / `"before"`)
- Appending to a file (`position: "append"`)
- Any file creation with more than trivial content

**Partial replacement workflow (existing file):**
1. `workslate_edit(name, file_path, old_string, new_string)` — load file from disk, apply edit, review diff
2. If more edits needed: `workslate_edit(name, old_string, new_string)` — edits buffer (no file_path = chains with previous)
3. `workslate_apply(name)` — uses stored file_path
4. `workslate_clear(name)` — clean up the buffer

**Full file workflow (new file):**
1. `workslate_write(name, content, file_path)` — draft the full content, review the returned diff
2. If issues found: `workslate_edit(name, old_string, new_string)` — edits buffer directly (no file_path = buffer mode)
3. `workslate_apply(name)` — uses stored file_path
4. `workslate_clear(name)` — clean up the buffer

`workslate_diff(name)` remains available for re-checking a buffer against its target file at any time. Use `workslate_diff(name, summary=true)` for a one-line stat ("N hunks, +X/-Y lines") to save context.

**Buffer dependencies:** `workslate_write(name, content, file_path, depends_on=["buf-a", "buf-b"])` declares that this buffer must be applied after buf-a and buf-b. `workslate_apply` enforces the ordering.

**Dry run:** `workslate_apply(name, dry_run=true)` shows the final file content with line numbers without writing to disk.

**Rules:**
- **Always pass `file_path` to `workslate_write`** so the diff is returned for review. Omitting it skips the review — only acceptable for scratch buffers not destined for files.
- Use descriptive buffer names that indicate the target (e.g., `auth-middleware`, `lock-ordering-fix`)
- Chain-of-thought prohibition applies equally to staged code — no reasoning in comments
- Clear buffers after applying to avoid stale state across tasks
- When working in Agent Teams, each teammate should use buffer names prefixed with their scope to avoid collisions

## Task Sessions

**`workslate_task_init(name)` is mandatory before using any task tool.** Tasks are stored in SQLite (`workslate.db`) and shared across all agent instances in the same project. This replaces built-in TaskCreate/TaskUpdate entirely.

**Namespaces:** Tasks use `ws:` (personal) or `team:` (team coordination) prefixes:
- `workslate_task_create("Fix auth", namespace="ws")` → creates `ws:1`
- `workslate_task_create("Port handlers", namespace="team", owner="backend-dev")` → creates `team:1`
- `workslate_task_done("team:1")` — ID format: `"3"` (defaults to ws), `"ws:3"`, or `"team:3"`

**Cross-namespace dependencies:** `depends_on: ["ws:1", "team:2"]` — a task can depend on tasks in either namespace.

**Footer** shows both namespaces: `── Tasks (session) ws:[3/5] team:[8/12] ──`

**Workflow:**
1. `workslate_task_init("auth-refactor")` — create or resume a named session
2. `workslate_task_create(name, namespace?, owner?, depends_on?)` — create tasks
3. `workslate_task_done("ws:1")` / `workslate_task_update("team:3", status="in_progress")` — update
4. `workslate_task_list(namespace?)` — list tasks, optional namespace filter
5. `workslate_task_sessions()` — list all sessions with per-namespace counters

**Rules:**
- `workslate_task_init` must be called before any task operation
- Only one session is active at a time per MCP server instance
- Switching sessions does NOT clear the previous session's tasks (SQLite persists)
- Restarting the MCP server clears the active session — call `workslate_task_init` again to resume
- Buffers are shared across sessions (not scoped)
- Multiple agent instances can read/write the same session concurrently (SQLite WAL mode)

## After Completion

- [ ] All deliverables complete
- [ ] No placeholders or TODOs remain
- [ ] Tests pass (if applicable) — **actually verified, not assumed**
- [ ] No regression in related features
- [ ] Linting/type checking passes (if applicable)
- [ ] Workslate buffers cleared (`workslate_clear`) — no stale state left behind
- [ ] Outcome reported faithfully — failures disclosed, not hidden
