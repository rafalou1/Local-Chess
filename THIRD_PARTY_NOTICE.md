# Third-party assets and notices

On 2026-10-14 the repository owner confirmed that they have permission to redistribute the third-party artwork, fonts, sounds and voice assets in this release. These assets retain their respective ownership and license terms; they are not relicensed under the application source's GPL license. This project does not claim provider or Chess.com endorsement. The historical provenance below records how these assets were acquired for the original local build; the earlier personal-use status has been superseded by the owner's permission confirmation.

# Asset provenance and licensing — 0.1.1

The application source license does not grant rights to third-party artwork, fonts or sounds. The original Chess.com assets were obtained for the user's requested personal local comparison build. The repository owner reports redistribution permission. Third-party assets retain their ownership and are not covered by the application source license.

| Material | Origin and treatment |
|---|---|
|Active Neo piece PNGs, Walnut board, David avatar, navigation artwork|Original official Chess.com CDN assets; owned by Chess.com; owner reports redistribution permission; ownership retained|
|Ten active move/game MP3 sounds|Original official Chess.com default sound files; owned by Chess.com; owner reports redistribution permission; ownership retained|
|ChessSans Bold/ExtraBold and ChessGlyph fonts|Official Chess.com font files; owner reports redistribution permission; ownership retained|
|Ten classification SVGs|Original SVG paths extracted as static data from Chess.com's public shared frontend bundle; viewBox adapted to the observed 18×19 rendering; no original JavaScript is bundled/executed; owner reports redistribution permission; ownership retained|
|Application source and original exercises|Project GPL-3.0-or-later; this does not relicense third-party assets|
|Stockfish 19 Windows universal binary|Official Stockfish release, GPL-3.0; complete supplied source, scripts, AUTHORS and Copying retained in source delivery; engine notices accompany portable build|
|shakmaty / pgn-reader|Locked upstream GPL-3.0-or-later crates|
|Lucide React icons|ISC notice in licenses/|
|React, Vite, Tauri and other locked dependencies|Upstream respective licenses; comprehensive transitive license audit remains pending|
|Windows speech|Installed Microsoft SAPI voices; no voice data copied into this project; different from Chess.com's David voice|
|Old SVG pieces, generated oak, procedural wood and Nunito Sans|Inactive legacy assets retained for traceability. Generated-wood prompt and earlier provenance retained in docs/archive/ASSET_LICENSES-0.1.0.md; Nunito OFL notice retained|
|User reference and authenticated site captures|Supporting comparison evidence, not rendered application backgrounds|

Every active reference asset's original URL, byte size and SHA-256 is in REFERENCE_ASSETS.json. Assets are bundled locally; ordinary play does not load CDN resources. Chess.com branding and source assets do not imply affiliation. The coaching text and classifier are local implementations, not Chess.com's proprietary coach or grading algorithm.

Stockfish binary SHA-256: 45bc8e4969147db9c2eb533810637994619bff0eacc81ccfd9854394901bcbd0. Official sources: https://stockfishchess.org/download/ and https://github.com/official-stockfish/Stockfish .

## 0.1.2 neural speech additions

Kokoro82M model license: Apache-2.0 (bundled LICENSE). Sherpa-onnx1.13.8: Apache-2.0, official source archive retained in third-party/voice-source. ONNX Runtime1.28.2: MIT, with full third-party notices in runtime resources. Espeak-ng phonemization: GPL-3.0, pinned corresponding source ed530aa113046142eb5115cf2fc9157854d0ffe1; piper-phonemize source f3ff95afc03640bc1399e113e83361192a2fafb4 and build scripts retained, hashes verified against upstream CMake. See third-party/voice-source/SOURCES.json. This does not certify every transitive dependency’s complete licensing audit.

Fast Piper models preserve their upstream MODEL_CARD and JSON configuration. Ryan’s training dataset is RyanSpeech, attributed to the source linked in MODEL_CARD and licensed CC BY-NC-SA4.0. Lessac’s dataset is the CSTR Blizzard2013 Lessac corpus, whose license is linked in MODEL_CARD. Model/dataset licenses are not relicensed by this application. The owner confirmed permission to redistribute the included models and reference assets. Their specific model/dataset terms remain applicable; the source GPL does not relicense them.

0.1.3 adds the observed official magnifier-star SVG and default player GIF; same personal-comparison scope and ownership. Provenance and hashes recorded in REFERENCE_ASSETS.json.

0.1.3 winner/mate SVG paths are static transcriptions of observed inline artwork, with local colored circular wrappers. No upstream JavaScript is executed or bundled.

## AI provider identity assets

Codex uses the unchanged icon from the official OpenAI.Codex Windows package (26.1007.9446.0). Claude uses the unchanged asterisk at https://claude.ai/favicon.svg. Antigravity uses Google's unchanged icon at https://antigravity.google/assets/image/brand/antigravity-icon__full-color.svg (official press kit: https://antigravity.google/press). Provider names/logos identify the selected tool and retain their owners' rights; the source GPL does not relicense them. No endorsement is claimed.
