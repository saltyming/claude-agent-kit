<!-- slate-agent-kit:common -->
# Framework Conventions

These are the project's language- and framework-specific conventions. Stack choices are project-owned; this file collects the patterns you follow on whichever stack the project uses.

## React / Next.js

**File Naming:**
- Components: PascalCase (`UserProfile.tsx`)
- Utilities: camelCase (`formatDate.ts`)
- Hooks: `use` prefix (`useAuth.ts`)
- Types: `.types.ts` or `.types.tsx`

**Component Structure:**
```tsx
// 1. Imports
// 2. Types
// 3. Component
// 4. Export
```

## Rust

**Error Handling:**
- Never use `.unwrap()` in production code

## Python

**Style:**
- Type hints required
- Docstrings for public APIs
