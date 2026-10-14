# Install Local Chess Coach

## Windows

1. [Download the installer](https://github.com/rafalou1/Local-Chess/releases/latest/download/LocalChessCoach-Windows-Setup.exe).
2. Run `LocalChessCoach-Windows-Setup.exe` and complete setup.
3. Launch **Local Chess Coach** from the desktop or Start menu.

Use Windows 10/11 on a 64-bit Intel or AMD computer. The installer includes the WebView2 offline installer. Local coaching, puzzles and included voices work offline after installation. AI opponents require internet, an official provider CLI and your provider account.

This release is unsigned, so Windows may show an **unrecognized app** message. Download only from this repository's release page. If you choose to proceed, Windows exposes **More info → Run anyway**. You can compare the file's SHA-256 against `SHA256SUMS.txt` on the release page:

```powershell
Get-FileHash "$env:USERPROFILE\Downloads\LocalChessCoach-Windows-Setup.exe" -Algorithm SHA256
```

## First game

Choose your side and press **Play** for local coaching. No additional tools or sign-in are needed.

For AI play, choose **AI provider**, select a provider and use **Install** or **Sign in** if shown. Choose a model, accept position sharing and press Play. Options contains effort and timeout settings. A slower reply can be cancelled, retried, or switched to local play.

## Updates

Close Local Chess Coach and run the new installer using the normal installation folder. Updates preserve the local database. Reopen the app to continue your saved game; provider games reopen paused.

## Saved games and backups

Data is stored in `%APPDATA%\org.localchess.coach`. Close the app before copying that folder to a backup location. Keep the whole folder together, including any SQLite WAL files. The usual installation folder is `%LOCALAPPDATA%\Programs\Local Chess Coach`.

## Troubleshooting

- **The download is large:** roughly 458 MB includes local engines, voices, puzzles and WebView2 so ordinary play works offline.
- **An AI provider is not ready:** install its official CLI, complete its official sign-in, then press the refresh button. Your provider account must have access to the chosen model.
- **AI replies take several seconds:** try lower effort or a faster available model. Model reasoning and service latency vary.
- **Report an issue:** [open an issue](https://github.com/rafalou1/Local-Chess/issues) with the app version, Windows version, provider/model if relevant and steps to reproduce. Keep passwords, keys, tokens and personal game details out of reports.
