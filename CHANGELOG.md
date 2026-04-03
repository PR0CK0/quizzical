# Changelog

All notable changes to this project will be documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

---

## [0.2.0] - 2026-04-03

### Added
- **Deathmatch mode** (`☠ DEATHMATCH`) — 5s/question, no result screens, inline flash feedback only
- **Beat mode** (`⚡ BEAT`) — reworked to 10s/question (was 20s)
- **Leaderboard** — top 10 beat/deathmatch scores saved to `~/Library/Application Support/quizzical/scores.json`; shown on final screen
- **Hard mode timer toggle** — title screen now cycles NORMAL → BEAT → DEATHMATCH with `←`/`→`
- **Deck metadata** — `"name"` field in JSON shown as `{name} edition` on title screen
- **3-row fuse animation** — ember glow above spark, main fuse line, smoke trail below
- **Deathmatch inline feedback** — 300ms CORRECT/WRONG flash on prompt row, immediately continues
- `dirs-next` dependency for platform-correct data directory

### Changed
- Scores saved outside the deck directory (no longer pollutes file picker)
- `GameMode::BeatHard` renamed to `GameMode::BeatHard` → displayed as "DEATHMATCH"
- Fuse spark upgraded to 3-row animated display with ember and smoke rows
- Beat mode final screen uses plain ASCII header (fixed Unicode centering bug)
- Title screen mode toggle cycles three options instead of two

### Fixed
- Beat mode final screen box misaligned due to Unicode box-drawing char byte length
- `scores.json` appearing in the deck file picker
- Escape on title screen now exits cleanly instead of starting the quiz

---

## [0.1.4] - 2026-04-03

### Added
- Demo screenshot (`demo.png`) in README

---

## [0.1.3] - 2026-04-03

### Fixed
- CI publish workflow: use `--locked` flag and `CARGO_REGISTRY_TOKEN` env var
- GitHub Actions now creates GitHub release automatically on version tag push

---

## [0.1.2] - 2026-04-03

### Added
- GitHub Actions workflow to auto-publish to crates.io on `v*` tag push

---

## [0.1.1] - 2026-04-03

### Added
- crates.io metadata in `Cargo.toml` (description, license, keywords, categories)
- README install instructions with `cargo install quizzical`
- crates.io badge in README

---

## [0.1.0] - 2026-04-03

### Added
- Initial release
- Full-screen terminal quiz game using `crossterm`
- CompTIA Security+ SY0-701 deck (333 questions)
- Acronym drill deck (100 questions, `security+ acronyms edition`)
- Normal mode — blocking ABCD quiz with particle explosion on correct answers
- Beat mode — burning fuse timer, speed multiplier (up to 5×), combo chain (up to 3×)
- Startup file picker (arrow keys) or `--file` CLI flag
- Deck `name` field shown as `{name} edition` on title screen
- Physics-based particle burst animation (gravity, trails, 44 particles)
- `fmt_score()` thousands-separator formatting
