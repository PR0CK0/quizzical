# quizzical — todo

## [x] separate quiz file loading
- cli arg: `./quizzical --file security-plus.json`
- startup picker if no arg given: list all `.json` files in the current dir, arrow keys to select
- each json file follows the same schema (`{ "name": "...", "questions": [...] }`)

## [x] acronym mode
- dedicated deck file: `acronyms.json`
- 100 cards: acronym → pick the correct expansion (ABCD)
- [ ] flip mode: shows the expansion → pick the acronym

## [x] beat mode
- timed mode — burning fuse counts down per question (default 20s)
- answer fast → higher point multiplier (up to 5×)
- combo chain: consecutive correct answers multiply score (up to 3×)
- fuse animation: fuel/spark/ash model, color shifts green → yellow → red → flicker
- combo row shows current streak + last points awarded
- timeout: "TIME'S UP" flash, combo reset
- wrong: "COMBO BROKEN" flash if streak active
- early quit → full stats screen
- [x] configurable time per question — HARD mode = 10s (toggle on title screen)
- [x] leaderboard saved to `scores.json` (top 10 runs, shown on final beat screen)

## [x] deck metadata (`name` field in JSON)
- title screen shows `{name} edition` pulled from the loaded deck file

## [ ] GitHub Pages hosting
- static site that renders deck JSON as a web-based version of the quiz
- could use plain HTML/JS — no framework needed
- hosted from `docs/` or a `gh-pages` branch
- link from README

## [ ] flip mode (acronyms)
- show the expansion → user picks the acronym from ABCD options
- toggle at deck-select or title screen
