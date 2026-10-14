# Changes

## 0.2.17 — CLI response viewer

- Open **Opponent options → View CLI response** to see the actual answer that chose the move in a compact dark card. Copy the answer or browse earlier responses using previous/next controls.
- Show the assistant's original response text, with no event envelopes, prompts, tool lists or diagnostic metadata. Only the final answer from a correction attempt is displayed.
- Support keyboard dismissal, focus restoration, narrow windows, and English/German labels. Response history stays in memory for the current app session.
- Preserve queued premoves through coach blunder warnings; resume them after Continue rather than silently discarding them.

Verified: 120 native tests pass. Real Antigravity click premoves and the delayed-warning regression were tested, along with ten existing premove cases. The previous v0.2.16 build fails the preserved-queue assertion in the same warning regression.

## 0.2.16 — Antigravity startup repair

- Removed hanging headless settings probes. Model and effort choices come from actual pinned model IDs in the native catalog.
- Reuse a private isolated CLI profile for the app session instead of repeating cold initialization for every command. Restricted chess-agent files remain in each invocation workspace.
- Read completed JSON metadata immediately and keep subprocess shutdown waits bounded. Retry timed-out account metadata once; preserve a healthy catalog when a refresh fails.
- Distinguish connection failures from sign-out, show Retry and the actual setup error, and explain why Play is blocked. Consent is still explicit. Show Starting while creating a game and prevent duplicate starts.
- Synchronize Antigravity effort changes with the corresponding model variant and pin that exact ID.

Verified: all six listed Antigravity model selections start games through the Play button; Gemini 3.8 Flash Low played c7c5 and d7d6 after human moves in 5.133 and 4.200 seconds. Missing-consent feedback and timeout/Retry states were exercised. All 120 native tests pass, including model-variant and bounded metadata-shutdown regressions. These startup checks do not claim real inference was tested on every listed model.

## 0.2.15

Custom dark provider menus, X cancel icons, background updater suppression for chess subprocesses, premove destination markers and queue/race repairs. Windows release includes local Stockfish coaching, offline puzzles and voices. See the README and installation guide.
