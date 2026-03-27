# MCCP v4 — Codebase Intelligence Implementation Progress

> **Created:** 2026-03-27
> **Status:** In Progress

---

## Task List

| # | Task | Status | Notes |
|---|------|--------|-------|
| 1 | Enhanced code_intel: Java, Go, C#, Kotlin tree-sitter support | [ ] | Extend `TreeSitterAnalyzer` |
| 2 | Framework detection system (Spring, Express, Django, etc.) | [ ] | New `framework_detector.rs` |
| 3 | Flow tracking (Controller → Service → Repository → Model) | [ ] | New `flow_tracker.rs` |
| 4 | Enhanced SymbolRef with line, column, context | [ ] | Schema update + reference collector |
| 5 | Global variable & annotation/decorator extraction | [ ] | code_intel enhancements |
| 6 | Lombok-style codegen pattern detection | [ ] | New heuristic in framework detector |
| 7 | Background indexing (non-blocking, parallel) | [ ] | Pipeline improvements |
| 8 | Project structure diagram generation | [ ] | New `structure.rs` in core |
| 9 | Comprehensive tests for all new features | [ ] | Tests per module |
| 10 | README update | [ ] | Document new features |

---

## Implementation Log

### 2026-03-27 — Initial Implementation

**Starting state:**
- TreeSitterAnalyzer supports Rust, TypeScript/JavaScript, Python
- CodeIntelSnapshot has symbols, call_edges, use_edges, import_edges
- SymbolRef has file, line, context but no column
- No framework detection
- No flow tracking
- No codegen detection
- Pipeline runs in foreground with hash-based change detection

**Changes made:**
- (will be filled as we go)
