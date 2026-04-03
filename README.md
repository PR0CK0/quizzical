# quizzical

A terminal quiz game for certification exam prep. Built in Rust with a full-screen TUI, physics-based animations, and a beat mode with a burning fuse timer and combo scoring.

## Install

```bash
cargo build --release
```

Binary lands at `target/release/quizzical`.

## Usage

```bash
# startup file picker (arrow keys to select, Enter to confirm)
./target/release/quizzical

# jump straight to a deck
./target/release/quizzical --file security-plus.json
./target/release/quizzical --file acronyms.json
```

From the title screen, use `←` / `→` (or `B` / `Tab`) to toggle between **NORMAL** and **⚡ BEAT** mode, then `Enter` to start.

## Decks

| File | Description |
|---|---|
| `security-plus.json` | 333 CompTIA Security+ SY0-701 ABCD questions |
| `acronyms.json` | 100 acronym flashcards (SPF, AES, SIEM, ...) |

### Adding a deck

Create any `.json` file in the same directory following this schema:

```json
{
  "name": "my deck",
  "questions": [
    {
      "domain": "Category Name",
      "question": "What does XYZ stand for?",
      "answers": {
        "A": "Option one",
        "B": "Option two",
        "C": "Option three",
        "D": "Option four"
      },
      "correct": "A",
      "explanation": "XYZ stands for option one because..."
    }
  ]
}
```

The `name` field appears on the title screen as `{name} edition`. The file picker lists all `.json` files in the current directory automatically.

## Modes

### Normal
Standard flashcard quiz. One question at a time, immediate feedback, particle explosion on correct answers.

### Beat ⚡
Timed mode. A burning fuse counts down per question (default 20s).

- **Answer fast** → higher point multiplier (up to 5×)
- **Build a streak** → combo multiplier (up to 3×)
- **Max score per question**: 1,500 pts (5× speed × 3× combo)
- Wrong answer or timeout resets your combo
- Quit early with `q` — you still get a full stats screen

## Controls

| Key | Action |
|---|---|
| `A` `B` `C` `D` | Select answer |
| `q` | Quit / end session |
| `←` `→` or `B` `Tab` | Toggle mode on title screen |
| `Enter` | Confirm on title screen |
