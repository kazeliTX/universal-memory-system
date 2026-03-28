# UMMS QA Test Plan & Acceptance Criteria

> Generated: 2026-03-28 | Status: Active

## 1. Coverage Gap Analysis

### Currently Covered
| Area | Tests | Framework |
|------|-------|-----------|
| HTTP API integration (10 suites) | `full_stack_e2e.rs` | tokio + tower |
| Storage layer isolation (5 suites) | `storage_e2e.rs` | tokio + tempfile |
| Chat API client (2 suites) | `chat/__tests__/api.test.ts` | vitest + jsdom |
| Dashboard API client (6 suites) | `dashboard/__tests__/api-client.test.ts` | vitest + jsdom |

### Gaps Identified
| Area | Priority | Risk |
|------|----------|------|
| Vue component rendering | P0 | UI regression undetectable |
| Composable logic (useEvents) | P0 | WebSocket reconnect bugs |
| Chat session persistence (localStorage) | P1 | Data loss on reload |
| Agent switching flow | P1 | State leakage between agents |
| Dashboard view data display | P1 | Numbers correct but UI broken |
| Search result contribution bars | P2 | Visual math errors |
| Mobile responsive layout | P2 | Sidebar toggle failures |

## 2. Acceptance Criteria

### Chat Client

#### AC-CHAT-01: Agent Selection
- [ ] All agents from API render as clickable cards
- [ ] Selected agent has `.active` CSS class
- [ ] Clicking agent emits `update:agent` with correct ID
- [ ] Agent avatar shows first character of name, uppercase

#### AC-CHAT-02: Session Management
- [ ] New session creates with `title: '新对话'` and empty messages
- [ ] Sessions persist to `localStorage` under key `umms-chat-sessions`
- [ ] Corrupted localStorage gracefully defaults to empty array
- [ ] Deleting current session selects next available for same agent
- [ ] Switching agents selects most recent session for that agent

#### AC-CHAT-03: Chat Window
- [ ] Empty state renders when no messages exist
- [ ] User message appears immediately in bubble (before API response)
- [ ] ThinkingIndicator shows during loading
- [ ] API error renders as assistant message with error text
- [ ] Enter sends message, Shift+Enter inserts newline
- [ ] Send button disabled when input empty or loading

#### AC-CHAT-04: Message Display
- [ ] User messages have `.message-user` class (right-aligned)
- [ ] Assistant messages have `.message-assistant` class + avatar
- [ ] Latency badge shows when `latency_ms` present
- [ ] Sources toggle reveals SourcePanel with correct count
- [ ] Session title auto-updates from first message content

#### AC-CHAT-05: WebSocket Events
- [ ] Connection indicator shows green dot when connected
- [ ] Events display badge count
- [ ] Auto-reconnect after 3 seconds on close
- [ ] Keeps max 100 events (FIFO)
- [ ] Malformed JSON messages silently ignored

### Dashboard

#### AC-DASH-01: Overview Page
- [ ] Health status renders badge per storage component
- [ ] Stats grid shows L0/L1 entries, vector total, graph nodes/edges
- [ ] Encoder card shows online/offline status with metrics
- [ ] Auto-refreshes every 5 seconds
- [ ] Connection error renders error card

#### AC-DASH-02: Memory Browser
- [ ] Agent selector switches data source
- [ ] Layer toggle (cache/vector) switches API endpoint
- [ ] Search-as-you-type triggers after 500ms debounce, 2+ chars
- [ ] Search results table shows score, source tag, contribution bar
- [ ] Contribution bar math: BM25% + Vector% = 100%
- [ ] EPA metrics display when available

#### AC-DASH-03: API Client Transport
- [ ] Detects Tauri environment via `__TAURI_INTERNALS__`
- [ ] Falls back to HTTP fetch when not in Tauri
- [ ] Error responses throw with HTTP status

## 3. Test Strategy

### Frontend (Vitest + Vue Test Utils)
```
chat/src/__tests__/
  api.test.ts           (existing)
  useEvents.test.ts     (NEW - composable logic)
  AgentSelector.test.ts (NEW - component rendering + events)
  ChatWindow.test.ts    (NEW - send flow + error handling)
  MessageBubble.test.ts (NEW - conditional rendering)
  App.test.ts           (NEW - session persistence + agent switching)

dashboard/src/__tests__/
  api-client.test.ts    (existing)
  Overview.test.ts      (NEW - health/stats display)
  MemoryBrowser.test.ts (NEW - search + debounce + contribution bars)
```

### What We Test at Each Layer
- **Component tests**: Rendering correctness, event emission, prop-driven display
- **Composable tests**: Reactive state management, WebSocket lifecycle
- **Integration tests** (Rust): API contract correctness, agent isolation, data flow

### What We Don't Test (by design)
- CSS visual appearance (colors, fonts) - manual review
- Third-party library internals (Naive UI rendering)
- Network timing / real WebSocket connections
