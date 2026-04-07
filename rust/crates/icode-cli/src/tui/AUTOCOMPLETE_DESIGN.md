# Autocomplete Design: Unified Dropdown Overlay

## Status

Design phase. No implementation code has been written.

## Problem

The current TUI uses two disconnected autocomplete systems that do not share code, state, or rendering:

1. **Inline slash completion** — renders directly below the input bar in `layout.rs` using `render_slash_autocomplete()`. Only handles `/` commands. State lives as six scattered fields on `InputState`.
2. **Dead file picker** — `file_picker.rs` has `FilePickerState`, `scan_files()`, `fuzzy_match()`, and `parse_file_references()` that are never wired into the TUI. The code compiles but is never called.

Meanwhile, `@` references (files, agents, resources) have no autocomplete at all. The user types `@foo` blind with no visual feedback.

The opencode reference implementation (`autocomplete.tsx`) uses a single unified dropdown that handles both `@` and `/` modes through one component. This design document specifies the same approach for icode.

## Current State

### What exists today

| Component | File | State |
|-----------|------|-------|
| Slash autocomplete (inline) | `input.rs` | Live — 6 fields on `InputState` |
| Slash dropdown renderer | `layout.rs` | Live — `render_slash_autocomplete()` |
| Slash key handler | `runner.rs` | Live — Tab/Up/Down/Enter in `handle_key()` |
| File picker (dead) | `file_picker.rs` | Dead code — never instantiated |
| Model picker (overlay) | `model_picker.rs` | Live — reference pattern for overlay rendering |
| Command palette | `command_palette.rs` | Live — separate overlay, triggered by Ctrl+P |

### Existing fields on `InputState` (input.rs, lines 26-40)

```
value: String                          // input text
cursor: usize                          // cursor position (char offset)
completions: Vec<String>               // generic completions (used by Tab cycling)
show_completions: bool                 // whether generic completions are active
completion_idx: usize                  // index into completions
show_slash_autocomplete: bool          // whether slash dropdown is visible
slash_completions: Vec<String>         // filtered slash commands
slash_completion_idx: usize            // selected index in slash_completions
```

### How slash autocomplete works today

1. User types `/` at position 0. `insert_char()` calls `update_slash_autocomplete()`.
2. `update_slash_autocomplete()` filters `SLASH_COMMANDS` by prefix match against the full input value. Sets `show_slash_autocomplete = true` if any match.
3. `layout.rs` checks `state.prompt.show_slash_autocomplete` after rendering the input box, then calls `render_slash_autocomplete()` which draws a bordered box directly above the input area.
4. In `runner.rs`, Tab/Up/Down/Enter check `show_slash_autocomplete` and delegate to the corresponding methods on `InputState`.
5. If `show_slash_autocomplete` is false but input starts with `/`, Tab falls through to `get_command_completions()` in `runner.rs` which does a separate, slightly different command list (includes `/theme`, excludes `/undo`/`/redo`).

### Inconsistencies in current implementation

- `SLASH_COMMANDS` in `input.rs` has 15 commands. `get_command_completions()` in `runner.rs` has 14 commands (different set). They should be the same source.
- `completions`/`show_completions`/`completion_idx` handle Tab cycling when the slash dropdown is closed. This is a second completion path that overlaps with the slash autocomplete.
- The inline renderer (`render_slash_autocomplete`) renders above the input box using `prompt_area.y.saturating_sub(height)`. It competes with the message area and can obscure content.
- No `@` autocomplete exists despite `file_picker.rs` having all the pieces ready.

### Model picker as overlay reference

`model_picker.rs` demonstrates the correct overlay pattern:
- Centered dialog with `Clear` widget to erase underlying content
- Responsive sizing based on terminal dimensions
- Search bar at top, scrollable list below, help text at bottom
- Section headers (Favorites, Recent, provider groups)
- `state.model_picker.open` gate in `layout.rs`

This same pattern should be reused for the autocomplete dropdown.

## Target State

### Unified autocomplete

A single `AutocompleteState` replaces all six completion-related fields on `InputState`. One overlay renderer handles all modes. One key handler routes navigation.

```
Before:                                    After:
InputState:                                InputState:
  show_slash_autocomplete: bool              trigger_autocomplete(mode, trigger_pos)
  slash_completions: Vec<String>             hide_autocomplete()
  slash_completion_idx: usize
  completions: Vec<String>
  show_completions: bool
  completion_idx: usize

AppState:                                  AppState:
  model_picker: ModelPickerState             model_picker: ModelPickerState
  command_palette: CommandPaletteState       command_palette: CommandPaletteState
                                             autocomplete: AutocompleteState   <-- NEW
```

### New data structures

```rust
/// What kind of autocomplete is active.
enum AutocompleteMode {
    /// Triggered by / at position 0. Shows commands.
    Slash,
    /// Triggered by @ at position 0 or after whitespace. Shows files.
    File,
    /// Triggered by @ followed by mode cycling (future). Shows agents.
    Agent,
    /// Triggered by @ followed by mode cycling (future). Shows MCP resources.
    Resource,
}

/// The category of a single autocomplete entry.
enum EntryKind {
    File,
    Command,
    Agent,
    Resource,
}

/// One row in the autocomplete dropdown.
struct AutocompleteEntry {
    /// Primary display text (e.g., "/help", "src/main.rs", "@researcher").
    title: String,
    /// Secondary hint text (e.g., "Show help", "1.2 KB", "Research agent").
    subtitle: String,
    /// What kind of entry this is, for icon/color differentiation.
    kind: EntryKind,
}

/// State for the unified autocomplete overlay.
struct AutocompleteState {
    /// Whether the overlay is currently visible.
    open: bool,
    /// Which mode is active (determines what entries are shown).
    mode: AutocompleteMode,
    /// Filtered list of entries matching the current query.
    entries: Vec<AutocompleteEntry>,
    /// Currently selected entry index.
    idx: usize,
    /// Scroll offset for entries that do not fit in the visible window.
    scroll: usize,
    /// Character position in the input where the trigger character (/ or @) was typed.
    /// For slash mode this is always 0. For @ mode this is the position of @.
    trigger_pos: usize,
}
```

### Migration mapping

| Old field | New location | Notes |
|-----------|-------------|-------|
| `InputState.show_slash_autocomplete` | `AutocompleteState.open` | Replaced by `open` with `mode: Slash` |
| `InputState.slash_completions` | `AutocompleteState.entries` | Migrated to structured entries |
| `InputState.slash_completion_idx` | `AutocompleteState.idx` | Direct mapping |
| `InputState.completions` | `AutocompleteState.entries` | Absorbed into unified entries |
| `InputState.show_completions` | `AutocompleteState.open` | No longer needed as separate flag |
| `InputState.completion_idx` | `AutocompleteState.idx` | Direct mapping |
| (new) | `AutocompleteState.mode` | Determines what entries to show |
| (new) | `AutocompleteState.trigger_pos` | Where to splice the selection back into input |
| (new) | `AutocompleteState.scroll` | Scroll offset for overflow |
| (new) | `AutocompleteState.entries[].subtitle` | Contextual hints per entry |
| (new) | `AutocompleteState.entries[].kind` | For visual differentiation |

### Trigger logic

The trigger detection moves from `InputState::update_slash_autocomplete()` into a new method that inspects each character as it is typed:

```
On character insert (c, cursor_position):
  if c == '/' and cursor_position == 1 (first char typed):
    open(AutocompleteMode::Slash, trigger_pos: 0)

  if c == '@' and (cursor_position == 1 or char_before_cursor_is_whitespace):
    open(AutocompleteMode::File, trigger_pos: cursor_position - 1)

On backspace:
  if cursor is at trigger_pos:
    close()

On space:
  if autocomplete is open and mode is Slash:
    close()   // slash commands cannot have spaces
  if autocomplete is open and mode is File:
    close()   // file paths cannot have spaces
```

The `update_slash_autocomplete()` method on `InputState` is replaced by a single `rebuild_entries()` method on `AutocompleteState` that filters the appropriate source based on `mode`:

```
AutocompleteState::rebuild_entries(&mut self, input: &str, cwd: &str):
  match self.mode:
    Slash:
      query = input (entire value, since trigger is always at 0)
      self.entries = SLASH_COMMANDS
        .iter()
        .filter(|cmd| cmd.starts_with(query))
        .map(|cmd| AutocompleteEntry {
          title: cmd.to_string(),
          subtitle: command_help_text(cmd),
          kind: EntryKind::Command,
        })
        .collect()

    File:
      query = input[self.trigger_pos + 1 .. self.cursor]  // text after @
      self.entries = fuzzy_match(scan_files(cwd), query)
        .iter()
        .map(|path| AutocompleteEntry {
          title: path.clone(),
          subtitle: "",   // could show file size in future
          kind: EntryKind::File,
        })
        .collect()

    Agent:
      ... (future)

    Resource:
      ... (future)

  self.idx = 0
  self.scroll = 0
```

### Key handling

The key handler in `runner.rs` currently checks `self.state.prompt.show_slash_autocomplete` in three places (Tab, Up, Down). This becomes a single gate:

```
Before (runner.rs):
  Tab:
    if show_slash_autocomplete:
      slash_autocomplete_select()
    else if input.starts_with('/'):
      get_command_completions() ...

After:
  Tab:
    if autocomplete.open:
      autocomplete.select()
    // (no fallback — the overlay handles all completions)

  Up:
    if autocomplete.open:
      autocomplete.cursor_up()
    else:
      ... existing history/navigation logic ...

  Down:
    if autocomplete.open:
      autocomplete.cursor_down()
    else:
      ... existing history/navigation logic ...

  Esc:
    if autocomplete.open:
      autocomplete.close()
      return None
    ... existing Esc logic ...

  Enter:
    if autocomplete.open:
      autocomplete.select()   // select and close, but don't submit
      return None
    ... existing submit logic ...
```

### Rendering approach

The autocomplete overlay follows the `model_picker.rs` pattern, not the current inline approach:

1. Rendered as a centered overlay below the cursor position, not above the input box.
2. Uses `Clear` widget to erase content beneath it.
3. Bordered panel with a max of 8 visible items, scrollable.
4. Appears below the cursor when there is room, above when the cursor is near the bottom of the screen.
5. Selected entry highlighted with theme primary color.
6. Entry kind shown as prefix icon: `/` for commands, file icon for files, `@` for agents/resources.

```
Rendering in layout.rs:

  // After rendering everything else, check for autocomplete overlay
  if state.autocomplete.open {
      render_autocomplete_overlay(frame, &mut state.autocomplete, area, theme);
  }
```

The overlay width adapts to the longest entry title, capped at 60 columns. Height adapts to the number of entries, capped at 8. Position is calculated from the cursor position in the input widget.

### What gets removed

| File | What to remove | Why |
|------|---------------|-----|
| `input.rs` | `completions`, `show_completions`, `completion_idx` fields | Absorbed into `AutocompleteState` |
| `input.rs` | `show_slash_autocomplete`, `slash_completions`, `slash_completion_idx` fields | Absorbed into `AutocompleteState` |
| `input.rs` | `update_slash_autocomplete()`, `hide_slash_autocomplete()`, `slash_autocomplete_up/down/select()`, `selected_slash_completion()`, `set_completions()`, `cycle_completion_forward/backward()` methods | Replaced by `AutocompleteState` methods |
| `input.rs` | `SLASH_COMMANDS` constant | Move to shared commands module or keep as `AutocompleteState` source |
| `layout.rs` | `render_slash_autocomplete()` function | Replaced by `render_autocomplete_overlay()` |
| `layout.rs` | Two call sites checking `show_slash_autocomplete` (line 84-86 and line 331-333) | Replaced by single `autocomplete.open` check |
| `runner.rs` | `get_command_completions()` function | Absorbed into `AutocompleteState::rebuild_entries()` |
| `runner.rs` | Slash-specific branches in Tab/Up/Down handlers | Replaced by `autocomplete.open` gate |

### What gets reused

| Existing code | How it is reused |
|---------------|-----------------|
| `SLASH_COMMANDS` constant | Source for `AutocompleteMode::Slash` entries |
| `render_slash_autocomplete()` filtering logic | Prefix filtering in `rebuild_entries()` |
| `file_picker.rs::scan_files()` | Source for `AutocompleteMode::File` entries |
| `file_picker.rs::fuzzy_match()` | Filtering for file mode entries |
| `model_picker.rs` overlay pattern | Rendering template for the dropdown |
| `model_picker.rs::compute_scroll_offset()` | Scroll offset logic for the dropdown |

### What gets repurposed

| Existing code | New role |
|---------------|----------|
| `file_picker.rs::FilePickerState` | Merged into `AutocompleteState` — the struct itself is removed, but its `scan_files()` and `fuzzy_match()` helpers stay |
| `file_picker.rs::parse_file_references()` | Called when user selects a file entry or submits the prompt, to extract `@path` references |
| `input.rs` command completion fallback in runner | Eliminated — the autocomplete overlay handles all completion paths |

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────┐
│  runner.rs: handle_key()                                │
│  ┌───────────────────────────────────────────────────┐  │
│  │  Key pressed                                      │  │
│  │    │                                              │  │
│  │    ├─ '/' at pos 0 → autocomplete.open(mode:Slash,trigger:0)
│  │    ├─ '@' at pos 0/after ws → autocomplete.open(mode:File,trigger:pos)
│  │    ├─ Backspace at trigger_pos → autocomplete.close()
│  │    ├─ Space → autocomplete.close()
│  │    ├─ Char → input.insert_char() → autocomplete.rebuild_entries()
│  │    ├─ Tab → if open: select() else: (removed)
│  │    ├─ Up/Down → if open: cursor_up/down() else: history/scroll
│  │    ├─ Enter → if open: select() else: submit()
│  │    └─ Esc → if open: close() else: (existing)
│  └───────────────────────────────────────────────────┘
└─────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────┐
│  AutocompleteState                                      │
│  ┌───────────────────────────────────────────────────┐  │
│  │  open: bool                                       │  │
│  │  mode: AutocompleteMode                           │  │
│  │  entries: Vec<AutocompleteEntry>                  │  │
│  │  idx: usize                                       │  │
│  │  scroll: usize                                    │  │
│  │  trigger_pos: usize                               │  │
│  │                                                   │  │
│  │  rebuild_entries(input, cwd)                      │  │
│  │    ├─ Slash: filter SLASH_COMMANDS by prefix      │  │
│  │    └─ File: scan_files() → fuzzy_match(query)     │  │
│  │                                                   │  │
│  │  select() → splice entry into input at trigger_pos│  │
│  │  cursor_up() / cursor_down()                      │  │
│  │  open(mode, trigger_pos)                          │  │
│  │  close()                                          │  │
│  └───────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────┐
│  layout.rs: render_autocomplete_overlay()               │
│  ┌───────────────────────────────────────────────────┐  │
│  │  Calculate position from cursor in input widget   │  │
│  │  Calculate size from longest entry (max 60 cols)  │  │
│  │  render Clear                                     │  │
│  │  render bordered block                            │  │
│  │  render entries with kind prefix + highlight      │  │
│  │  scroll window around idx                         │  │
│  └───────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
```

## File-by-File Changes

### `input.rs`

**Remove:**
- Fields: `completions`, `show_completions`, `completion_idx`, `show_slash_autocomplete`, `slash_completions`, `slash_completion_idx`
- Methods: `update_slash_autocomplete()`, `hide_slash_autocomplete()`, `slash_autocomplete_up()`, `slash_autocomplete_down()`, `slash_autocomplete_select()`, `selected_slash_completion()`, `set_completions()`, `cycle_completion_forward()`, `cycle_completion_backward()`
- Constant: `SLASH_COMMANDS` (move to `autocomplete.rs`)

**Modify:**
- `insert_char()`, `insert_str()`, `backspace()`, `delete()`, `delete_word_left()`, `delete_to_start()`, `delete_to_end()`, `delete_word_right()`, `clear()`, `submit()` — remove calls to `update_slash_autocomplete()` and `hide_slash_autocomplete()`. These methods become simple text manipulation.

### `runner.rs`

**Remove:**
- `get_command_completions()` function
- Slash-specific branches in Tab handler (the fallback to `set_completions` + `cycle_completion_forward`)
- `show_slash_autocomplete` checks in Up/Down handlers

**Modify:**
- Tab: `if self.state.autocomplete.open { self.state.autocomplete.select() }`
- Up: `if self.state.autocomplete.open { self.state.autocomplete.cursor_up() }`
- Down: `if self.state.autocomplete.open { self.state.autocomplete.cursor_down() }`
- Esc: `if self.state.autocomplete.open { self.state.autocomplete.close(); return None; }`
- Enter: `if self.state.autocomplete.open { self.state.autocomplete.select(); return None; }`
- Character insert: after `self.state.prompt.insert_char(c)`, call `self.state.autocomplete.on_char_insert(c, cursor)` to detect triggers

### `layout.rs`

**Remove:**
- `render_slash_autocomplete()` function
- Two call sites (lines 84-86 and 331-333)

**Add:**
- `render_autocomplete_overlay()` — new overlay renderer following model_picker pattern

**Modify:**
- In `render_ui()`: replace slash autocomplete checks with `if state.autocomplete.open { render_autocomplete_overlay(...) }`

### `file_picker.rs`

**Keep:**
- `scan_files()` — public helper, used by `AutocompleteState::rebuild_entries()` in File mode
- `fuzzy_match()` — public helper, used by `AutocompleteState::rebuild_entries()` in File mode
- `read_file_content()` — utility, used elsewhere
- `parse_file_references()` — called on prompt submit to extract @ references
- `ParsedFileRef` struct — used by prompt processing

**Remove:**
- `FilePickerState` struct and all its methods — absorbed into `AutocompleteState`

### New file: `autocomplete.rs`

```
src/tui/autocomplete.rs

- AutocompleteMode enum
- EntryKind enum
- AutocompleteEntry struct
- AutocompleteState struct with all methods
- SLASH_COMMANDS constant (moved from input.rs)
- render_autocomplete_overlay() function (or in layout.rs)
```

## Entry Visual Design

Each entry in the dropdown shows:

```
  /help          Show help and available commands
▸ /model         Show or switch current model
  /compact       Compact conversation context
  /clear         Clear the current conversation
  /permissions   Show or switch permission mode
  ...
```

For file mode:

```
  src/main.rs
▸ src/tui/input.rs
  Cargo.toml
  README.md
  ...
```

The prefix character indicates selection state. The entry kind determines the text color for the title (command names use one color, file paths use another, per the existing theme system).

## Selection Behavior

When the user selects an entry (Tab or Enter):

**Slash mode:**
- Replace entire input with the selected command.
- Close the overlay.
- Cursor moves to end of the command text.

**File mode:**
- Replace text from `trigger_pos` to current cursor with `@<selected_path> `.
- Close the overlay.
- Cursor moves to after the inserted reference plus trailing space.

This matches the opencode reference behavior where `insertPart()` replaces the range from the trigger position to the current cursor.

## Future Extensions (Out of Scope)

- **Agent mode** (`@` cycling to agent selection) — requires agent registry integration.
- **Resource mode** (`@` cycling to MCP resources) — requires MCP resource listing.
- **Frecency sorting** — opencode tracks recently used files and boosts their rank. Not needed for v1.
- **Mouse support** — clicking entries to select. The model picker does not support this today.
- **Fuzzy matching for commands** — currently uses prefix matching. Could use substring or fuzzy matching for commands too.
- **Inline preview** — showing file content preview alongside the file entry (like opencode's virtual extmarks).
