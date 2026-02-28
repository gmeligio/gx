---
name: gx-code-patterns
description: Use when writing or modifying gx source code to follow established patterns for domain types, dependency injection, error handling, file I/O, and YAML extraction
---

# gx Code Patterns

## Domain Types

Use strong types to prevent value mix-ups — action identifiers, versions, and commit SHAs are distinct types, never raw strings. Parsed workflow data passes through an interpretation step that normalizes versions and separates pinned SHAs from version tags. A composite key type centralizes the `action@version` format. Resolution outcomes are an enum with three variants: resolved, corrected (SHA didn't match its version comment), or unresolved.

## Dependency Injection

The application entry point inspects environment (manifest presence) and injects concrete implementations. Command handlers are generic over store traits, so the same logic runs against file-backed or memory-only stores without branching.

## Error Handling

- Missing credentials degrade gracefully with warnings rather than hard failures

## File I/O

Stores track a dirty flag and only write on explicit save, making operations idempotent. Memory-backed stores have no-op saves — same interface, no side effects.

## YAML Version Extraction

YAML parsers strip comments, but version hints live in comments (`uses: action@SHA # v4`). The solution is a two-phase approach: first scan raw text to capture comment-embedded version hints, then parse YAML structurally and merge the two. The interpretation step resolves the combined data into normalized, structured refs.
