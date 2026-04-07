# Framework Conventions

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

**Naming:**
- Types/Structs: PascalCase
- Functions/Variables: snake_case
- Constants: SCREAMING_SNAKE_CASE

**Error Handling:**
- Use `Result<T, E>` for fallible operations
- Use `Option<T>` for nullable values
- Never use `.unwrap()` in production code
- Provide meaningful error context

## Python

**Style:**
- Follow PEP 8
- Type hints required
- Docstrings for public APIs
- `f-strings` for string formatting
