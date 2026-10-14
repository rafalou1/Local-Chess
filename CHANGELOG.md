# Changes

## 0.2.17 — Raw CLI response viewer

- Open **Opponent options → View raw response** to inspect the original CLI event stream, copy it, and view the exact sent position prompt. Browse responses from this game and inspect correction attempts separately.
- The compact dialog supports keyboard navigation, narrow windows, and English/German labels. Responses remain in memory for this app session; captured output is limited to 256 KiB per attempt, with truncation visibly marked.
- Preserve queued premoves through coach blunder warnings; resume them after Continue rather than silently discarding them.

Verified: 121 native tests pass. A real Antigravity turn accepted a click premove and the viewer matched the captured CLI output exactly. UI checks cover raw-text escaping, correction attempts, copy, keyboard focus, and narrow windows. The delayed blunder-warning regression and ten existing premove cases pass.

## 0.2.16 — Antigravity startup repair

- Removed hanging headless settings probes. Model and effort choices come from actual pinned model IDs in the native catalog.
- Reuse a private isolated CLI profile for the app session instead of repeating cold initialization for every command. Restricted chess-agent files remain in each invocation workspace.
- Read completed JSON metadata immediately and keep subprocess shutdown waits bounded. Retry timed-out account metadata once; preserve a healthy catalog when a refresh fails.
- Distinguish connection failures from sign-out, show Retry and the actual setup error, and explain why Play is blocked. Consent is still explicit. Show Starting while creating a game and prevent duplicate starts.
- Synchronize Antigravity effort changes with the corresponding model variant and pin that exact ID.

Verified: all six listed Antigravity model selections start games through the Play button; Gemini 3.8 Flash Low played c7c5 and d7d6 after human moves in 5.133 and 4.200 seconds. Missing-consent feedback and timeout/Retry states were exercised. All 120 native tests pass, including model-variant and bounded metadata-shutdown regressions. These startup checks do not claim real inference was tested on every listed model.

## 0.2.15

Custom dark provider menus, X cancel icons, background updater suppression for chess subprocesses, premove destination markers and queue/race repairs. Windows release includes local Stockfish coaching, offline puzzles and voices. See the README and installation guide.
