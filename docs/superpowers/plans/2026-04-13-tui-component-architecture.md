# TUI Component Architecture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the monolithic modal dispatch system (21 copy-paste `handle_*_from_modal` methods in a 3993-line runner.rs) with a Component trait pattern that eliminates boilerplate, wires up the existing but unused KeybindRegistry, and keeps all existing functionality intact.

**Architecture:** Introduce a `Component` trait with `handle_key()`, `render()`, `on_mount()`, and `on_unmount()` methods. Replace the `ActiveModal` enum + match dispatch with a `ModalStack` of `Box<dyn Component>`. Each dialog already has `cursor_up/down`, `type_char`, `backspace` methods — these become the Component's internal behavior. The KeybindRegistry (already fully implemented in `keybinds.rs`) gets wired into the dispatch layer.

**Tech Stack:** ratatui 0.29, crossterm 0.28, existing icode-cli crate (no new deps). All changes are internal restructuring — no public API changes.

---

## File Structure

### New Files
| File | Responsibility |
|---|---|
| `rust/crates/icode-cli/src/tui/component.rs` | `Component` trait, `ComponentAction` enum, `ModalStack` struct |
| `rust/crates/icode-cli/src/tui/components/mod.rs` | Re-exports all component implementations |
| `rust/crates/icode-cli/src/tui/components/list_picker.rs` | Generic list-picker component (replaces shared Esc/Up/Down/Enter/Char/Backspace pattern across 15+ dialogs) |
| `rust/crates/icode-cli/src/tui/components/model_picker.rs` | Model picker as Component (wraps existing ModelPickerState) |
| `rust/crates/icode-cli/src/tui/components/command_palette.rs` | Command palette as Component |
| `rust/crates/icode-cli/src/tui/components/mcp_dialog.rs` | MCP dialog as Component |
| `rust/crates/icode-cli/src/tui/components/skills_dialog.rs` | Skills dialog as Component |
| `rust/crates/icode-cli/src/tui/components/sessions_dialog.rs` | Sessions dialog as Component |
| `rust/crates/icode-cli/src/tui/components/theme_list.rs` | Theme list dialog as Component |
| `rust/crates/icode-cli/src/tui/components/plugins_dialog.rs` | Plugins dialog as Component |
| `rust/crates/icode-cli/src/tui/components/help_dialog.rs` | Help dialog as Component |
| `rust/crates/icode-cli/src/tui/components/context_viz.rs` | Context viz dialog as Component |
| `rust/crates/icode-cli/src/tui/components/session_branching.rs` | Session branching as Component |
| `rust/crates/icode-cli/src/tui/components/prompt_stash.rs` | Prompt stash as Component |
| `rust/crates/icode-cli/src/tui/components/export_options.rs` | Export options as Component |
| `rust/crates/icode-cli/src/tui/components/debug_panel.rs` | Debug panel as Component |
| `rust/crates/icode-cli/src/tui/components/provider_dialog.rs` | Provider dialog as Component |
| `rust/crates/icode-cli/src/tui/components/workspace_dialog.rs` | Workspace dialog as Component |
| `rust/crates/icode-cli/src/tui/components/message_action.rs` | Message action dialog as Component |

### Modified Files
| File | Change |
|---|---|
| `rust/crates/icode-cli/src/tui/mod.rs` | Add `pub mod component`, `pub mod components` |
| `rust/crates/icode-cli/src/tui/modal_manager.rs` | Replace `ActiveModal` enum with thin wrapper or delete entirely |
| `rust/crates/icode-cli/src/tui/runner.rs` | **Massive reduction**: delete 21 `handle_*_from_modal` methods, replace `handle_key` modal dispatch with `self.modal_stack.handle_key()`, wire KeybindRegistry |
| `rust/crates/icode-cli/src/tui/app.rs` | Replace `active_modal: Option<ActiveModal>` with `modal_stack: ModalStack`, update all `open_*` methods to push components, update `close_modal` to pop |
| `rust/crates/icode-cli/src/tui/layout.rs` | Replace `ActiveModal` match with `state.modal_stack.render(frame, theme)` |
| `rust/crates/icode-cli/src/tui/keybinds.rs` | No changes needed — already complete |

### Unchanged Files
All dialog `dialog_*.rs` state structs remain exactly as-is. They gain an `impl Component for XyzState` block but their existing methods (`cursor_up`, `type_char`, `backspace`, `open`, `close`, `toggle_server`, etc.) are reused directly. Render functions (`render_*_dialog`) also remain unchanged — they get called by the Component's `render()` method.

---

## Architecture Detail: Component Trait

```rust
// rust/crates/icode-cli/src/tui/component.rs

use crate::tui::keybinds::{KeyAction, KeybindRegistry};
use crate::tui::Theme;
use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;

/// What a component can request after handling a key.
pub enum ComponentAction {
    /// No special action — keep the component open.
    None,
    /// Close this component (pop from modal stack).
    Close,
    /// Close and return a value to the main loop (e.g. "/model sonnet").
    CloseWithValue(String),
}

/// A self-contained UI component that can be pushed onto the modal stack.
///
/// Each dialog/picker/overlay becomes one impl of this trait.
/// The trait eliminates the 21 copy-paste handle_*_from_modal methods.
pub trait Component {
    /// Handle a key event. Returns an action indicating what to do next.
    fn handle_key(
        &mut self,
        key: KeyEvent,
        app: &mut AppState,
        registry: &KeybindRegistry,
    ) -> ComponentAction;

    /// Render this component within the given area.
    fn render(&self, frame: &mut Frame, area: Rect, theme: Theme);

    /// Called when the component is pushed onto the modal stack.
    /// Use for initialization that requires access to AppState (loading data, etc.).
    fn on_mount(&mut self, _app: &mut AppState) {}

    /// Called when the component is popped from the modal stack.
    /// Use for cleanup or persisting state back to AppState.
    fn on_unmount(&mut self, _app: &mut AppState) {}

    /// Whether this component blocks all other input (like permission prompts).
    fn is_blocking(&self) -> bool { false }

    /// Whether this is a picker-style component (affects backdrop rendering).
    fn is_picker(&self) -> bool { false }

    /// Human-readable name for debugging.
    fn name(&self) -> &'static str;
}
```

## Architecture Detail: ModalStack

```rust
pub struct ModalStack {
    stack: Vec<Box<dyn Component>>,
}

impl ModalStack {
    pub fn new() -> Self { Self { stack: Vec::new() } }

    pub fn is_empty(&self) -> bool { self.stack.is_empty() }

    pub fn top(&self) -> Option<&dyn Component> {
        self.stack.last().map(|c| c.as_ref())
    }

    pub fn top_mut(&mut self) -> Option<&mut Box<dyn Component>> {
        self.stack.last_mut()
    }

    pub fn push(&mut self, component: Box<dyn Component>, app: &mut AppState) {
        let mut c = component;
        c.on_mount(app);
        self.stack.push(c);
    }

    /// Handle a key event on the topmost component.
    /// Returns Some(value) if the component requested CloseWithValue.
    pub fn handle_key(
        &mut self,
        key: KeyEvent,
        app: &mut AppState,
        registry: &KeybindRegistry,
    ) -> Option<String> {
        let Some(top) = self.stack.last_mut() else {
            return None;
        };

        let action = top.handle_key(key, app, registry);
        match action {
            ComponentAction::None => None,
            ComponentAction::Close => {
                if let Some(mut c) = self.stack.pop() {
                    c.on_unmount(app);
                }
                None
            }
            ComponentAction::CloseWithValue(v) => {
                if let Some(mut c) = self.stack.pop() {
                    c.on_unmount(app);
                }
                Some(v)
            }
        }
    }

    /// Clear all modals.
    pub fn clear(&mut self, app: &mut AppState) {
        while let Some(mut c) = self.stack.pop() {
            c.on_unmount(app);
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, theme: Theme) {
        for component in &self.stack {
            component.render(frame, area, theme);
        }
    }

    pub fn is_blocking(&self) -> bool {
        self.stack.last().is_some_and(|c| c.is_blocking())
    }
}
```

## Architecture Detail: Generic ListPicker Component

Most dialogs share the exact same key handling pattern:
```
Esc → close
Up → cursor_up
Down → cursor_down
Enter → confirm (dialog-specific)
Char(c) → type_char(c)
Backspace → backspace
```

The `ListPicker` struct captures this once:

```rust
pub struct ListPicker<S, F> {
    state: S,
    on_confirm: F,
    on_type_char: fn(&mut S, char),
    on_backspace: fn(&mut S),
    on_cursor_up: fn(&mut S),
    on_cursor_down: fn(&mut S),
    title: &'static str,
    render_fn: fn(&S, &mut Frame, Rect, Theme),
}
```

For simple dialogs like Help, Skills, ThemeList — this replaces ~30 lines of copy-paste in runner.rs with one constructor call.

For complex dialogs (ModelPicker, CommandPalette, Permission) — write a dedicated `impl Component` that handles their unique logic.

---

### Task 1: Component trait + ModalStack foundation

**Files:**
- Create: `rust/crates/icode-cli/src/tui/component.rs`
- Modify: `rust/crates/icode-cli/src/tui/mod.rs`

- [ ] **Step 1: Create the Component trait and ModalStack**

Create `rust/crates/icode-cli/src/tui/component.rs`:

```rust
use crate::tui::app::AppState;
use crate::tui::keybinds::KeybindRegistry;
use crate::tui::Theme;
use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;

/// What a component can request after handling a key.
pub enum ComponentAction {
    /// No special action — keep the component open.
    None,
    /// Close this component (pop from modal stack).
    Close,
    /// Close and return a value to the main loop.
    CloseWithValue(String),
}

/// A self-contained UI component for the modal stack.
pub trait Component {
    fn handle_key(
        &mut self,
        key: KeyEvent,
        app: &mut AppState,
        registry: &KeybindRegistry,
    ) -> ComponentAction;

    fn render(&self, frame: &mut Frame, area: Rect, theme: Theme);

    fn on_mount(&mut self, _app: &mut AppState) {}

    fn on_unmount(&mut self, _app: &mut AppState) {}

    fn is_blocking(&self) -> bool { false }

    fn is_picker(&self) -> bool { false }

    fn name(&self) -> &'static str;
}

/// A stack of modal components. Replaces Option<ActiveModal>.
pub struct ModalStack {
    stack: Vec<Box<dyn Component>>,
}

impl ModalStack {
    pub fn new() -> Self { Self { stack: Vec::new() } }

    pub fn is_empty(&self) -> bool { self.stack.is_empty() }

    pub fn push(&mut self, component: Box<dyn Component>, app: &mut AppState) {
        let mut c = component;
        c.on_mount(app);
        self.stack.push(c);
    }

    pub fn handle_key(
        &mut self,
        key: KeyEvent,
        app: &mut AppState,
        registry: &KeybindRegistry,
    ) -> Option<String> {
        let Some(top) = self.stack.last_mut() else {
            return None;
        };

        let action = top.handle_key(key, app, registry);
        match action {
            ComponentAction::None => None,
            ComponentAction::Close => {
                if let Some(mut c) = self.stack.pop() {
                    c.on_unmount(app);
                }
                None
            }
            ComponentAction::CloseWithValue(v) => {
                if let Some(mut c) = self.stack.pop() {
                    c.on_unmount(app);
                }
                Some(v)
            }
        }
    }

    pub fn clear(&mut self, app: &mut AppState) {
        while let Some(mut c) = self.stack.pop() {
            c.on_unmount(app);
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, theme: Theme) {
        for component in &self.stack {
            component.render(frame, area, theme);
        }
    }

    pub fn is_blocking(&self) -> bool {
        self.stack.last().is_some_and(|c| c.is_blocking())
    }

    pub fn depth(&self) -> usize { self.stack.len() }
}
```

- [ ] **Step 2: Update mod.rs exports**

Modify `rust/crates/icode-cli/src/tui/mod.rs` — add these lines after existing `pub mod` declarations:

```rust
pub mod component;
pub mod components;

pub use component::{Component, ComponentAction, ModalStack};
```

- [ ] **Step 3: Create components/mod.rs placeholder**

Create `rust/crates/icode-cli/src/tui/components/mod.rs`:

```rust
// Component implementations — each dialog gets one impl of the Component trait.
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p icode-cli 2>&1 | head -40`
Expected: errors about AppState not having `modal_stack` field (we'll fix that in Task 2). The component.rs file itself should compile fine.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/icode-cli/src/tui/component.rs rust/crates/icode-cli/src/tui/mod.rs rust/crates/icode-cli/src/tui/components/mod.rs
git commit -m "refactor(tui): add Component trait and ModalStack foundation"
```

---

### Task 2: Wire ModalStack into AppState

**Files:**
- Modify: `rust/crates/icode-cli/src/tui/app.rs`

- [ ] **Step 1: Replace `active_modal: Option<ActiveModal>` with `modal_stack: ModalStack`**

In `rust/crates/icode-cli/src/tui/app.rs`, add the import at the top:

```rust
use crate::tui::component::ModalStack;
```

In the `AppState` struct, replace:
```rust
pub active_modal: Option<ActiveModal>,
```
with:
```rust
pub modal_stack: ModalStack,
```

In `AppState::new()`, replace any initialization of `active_modal` (there isn't one — it defaults to None) — just ensure `modal_stack: ModalStack::new()` is present.

- [ ] **Step 2: Update `close_modal()` method**

Replace:
```rust
pub fn close_modal(&mut self) {
    self.active_modal = None;
}
```

With:
```rust
pub fn close_modal(&mut self) {
    self.modal_stack.clear(self);
}
```

- [ ] **Step 3: Update `is_any_modal_open()` and `is_modal_blocking()`**

Replace:
```rust
pub fn is_any_modal_open(&self) -> bool {
    self.active_modal.is_some()
}

pub fn is_modal_blocking(&self) -> bool {
    self.active_modal
        .as_ref()
        .is_some_and(ActiveModal::is_blocking)
}
```

With:
```rust
pub fn is_any_modal_open(&self) -> bool {
    !self.modal_stack.is_empty()
}

pub fn is_modal_blocking(&self) -> bool {
    self.modal_stack.is_blocking()
}
```

- [ ] **Step 4: Update all `open_*` methods to push components**

Each `open_*` method currently does something like:
```rust
pub fn open_model_picker(&mut self) {
    self.model_picker.open();
    self.active_modal = Some(ActiveModal::ModelPicker(std::mem::take(&mut self.model_picker)));
}
```

These will temporarily break because we haven't created the component implementations yet. For now, replace ALL `open_*` methods with stub implementations that push a placeholder. We'll implement the real components in subsequent tasks.

Add this temporary import:
```rust
use crate::tui::component::{Component, ComponentAction, ModalStack};
use crate::tui::keybinds::KeybindRegistry;
```

Replace ALL these methods in AppState:
- `open_permission`
- `open_question`
- `open_model_picker`
- `open_command_palette`
- `open_mcp`
- `open_skills`
- `open_theme_list`
- `open_plugins`
- `open_sessions`
- `open_message_action`
- `open_help`
- `open_context_viz`
- `open_session_branching`
- `open_prompt_stash`
- `open_export_options`
- `open_debug_panel`
- `open_provider`
- `open_workspace`
- `open_diff_view`
- `open_pager`
- `open_autocomplete`

With stub implementations. Example for `open_model_picker`:
```rust
pub fn open_model_picker(&mut self) {
    self.model_picker.open();
    // TODO: replaced by Component impl in Task 4
    self.active_modal = Some(ActiveModal::ModelPicker(std::mem::take(&mut self.model_picker)));
}
```

Keep the `ActiveModal` pattern temporarily — we'll migrate each dialog one at a time in Tasks 3-6. The ModalStack exists alongside ActiveModal during the transition.

- [ ] **Step 5: Add modal_stack field to AppState**

Add to the `AppState` struct definition (around line 228, near `active_modal`):
```rust
pub modal_stack: ModalStack,
```

Add to `AppState::new()` initialization (around line 363):
```rust
modal_stack: ModalStack::new(),
```

- [ ] **Step 6: Verify compilation**

Run: `cargo check -p icode-cli 2>&1 | head -40`
Expected: Should compile cleanly (ActiveModal still in use during transition).

- [ ] **Step 7: Commit**

```bash
git add rust/crates/icode-cli/src/tui/app.rs
git commit -m "refactor(tui): add ModalStack to AppState alongside ActiveModal (transition)"
```

---

### Task 3: Implement first Component — HelpDialog (simple pattern)

**Files:**
- Create: `rust/crates/icode-cli/src/tui/components/help_dialog.rs`
- Modify: `rust/crates/icode-cli/src/tui/components/mod.rs`
- Modify: `rust/crates/icode-cli/src/tui/app.rs`

- [ ] **Step 1: Create the HelpDialog Component**

Create `rust/crates/icode-cli/src/tui/components/help_dialog.rs`:

```rust
use crate::tui::app::AppState;
use crate::tui::component::{Component, ComponentAction};
use crate::tui::dialog_help::HelpDialogState;
use crate::tui::dialog_help::render_help_dialog;
use crate::tui::keybinds::{KeyAction, KeybindRegistry};
use crate::tui::Theme;
use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;

pub struct HelpDialogComponent {
    state: HelpDialogState,
}

impl HelpDialogComponent {
    pub fn new(mut state: HelpDialogState) -> Self {
        state.open();
        Self { state }
    }
}

impl Component for HelpDialogComponent {
    fn handle_key(
        &mut self,
        key: KeyEvent,
        _app: &mut AppState,
        registry: &KeybindRegistry,
    ) -> ComponentAction {
        if registry.matches(&KeyAction::DialogCancel, &key) {
            return ComponentAction::Close;
        }
        if registry.matches(&KeyAction::DialogUp, &key) {
            self.state.cursor_up(100); // Help has fixed number of sections
        }
        if registry.matches(&KeyAction::DialogDown, &key) {
            self.state.cursor_down(100);
        }
        // Page navigation
        if registry.matches(&KeyAction::DialogPageUp, &key) {
            self.state.cursor = self.state.cursor.saturating_sub(10);
        }
        if registry.matches(&KeyAction::DialogPageDown, &key) {
            self.state.cursor += 10;
        }
        ComponentAction::None
    }

    fn render(&self, frame: &mut Frame, area: Rect, theme: Theme) {
        render_help_dialog(frame, &self.state, area, theme);
    }

    fn name(&self) -> &'static str { "HelpDialog" }
}
```

- [ ] **Step 2: Register in components/mod.rs**

Update `rust/crates/icode-cli/src/tui/components/mod.rs`:

```rust
pub mod help_dialog;

pub use help_dialog::HelpDialogComponent;
```

- [ ] **Step 3: Wire into AppState::open_help**

In `rust/crates/icode-cli/src/tui/app.rs`, find `open_help()` and replace:

```rust
use crate::tui::components::HelpDialogComponent;

// ... in AppState:

pub fn open_help(&mut self) {
    self.help_dialog.open();
    let component = HelpDialogComponent::new(std::mem::take(&mut self.help_dialog));
    self.modal_stack.push(Box::new(component), self);
}
```

- [ ] **Step 4: Add handler in runner.rs**

In `rust/crates/icode-cli/src/tui/runner.rs`, in the `handle_key` method's modal dispatch match, add the new pattern. Keep the existing ActiveModal dispatch — we're doing a gradual migration. Add this BEFORE the `ActiveModal` match:

```rust
// Component stack takes priority
if let Some(result) = self.state.modal_stack.handle_key(key, &mut self.state, &self.keybinds) {
    return Some(result);
}
```

This requires adding `keybinds: KeybindRegistry` to the `Tui` struct:
```rust
pub struct Tui {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    event_loop: EventLoop,
    state: AppState,
    theme: Theme,
    turn_rx: Option<Receiver<TurnEvent>>,
    skill_manager: Arc<SkillManager>,
    pending_palette_action: Option<PaletteAction>,
    keybinds: KeybindRegistry,  // ADD THIS
}
```

In `Tui::new()`, initialize it:
```rust
let mut keybinds = KeybindRegistry::new();
keybinds.populate_defaults();
```

And add to the struct construction:
```rust
Ok(Self {
    terminal,
    event_loop,
    state,
    theme,
    turn_rx: None,
    skill_manager,
    pending_palette_action: None,
    keybinds,  // ADD THIS
})
```

- [ ] **Step 5: Verify compilation and test**

Run: `cargo check -p icode-cli 2>&1 | head -40`
Expected: Clean compilation.

Run: `cargo build -p icode-cli 2>&1 | tail -5`
Expected: Successful build.

- [ ] **Step 6: Commit**

```bash
git add rust/crates/icode-cli/src/tui/components/help_dialog.rs rust/crates/icode-cli/src/tui/components/mod.rs rust/crates/icode-cli/src/tui/app.rs rust/crates/icode-cli/src/tui/runner.rs
git commit -m "refactor(tui): migrate HelpDialog to Component pattern, wire KeybindRegistry"
```

---

### Task 4: Migrate list-picker dialogs (10 dialogs, same pattern)

These 10 dialogs all share the identical key handling pattern (Esc/Up/Down/Char/Backspace/Enter):
- MCP Dialog
- Skills Dialog
- Theme List Dialog
- Plugins Dialog
- Prompt Stash
- Export Options
- Debug Panel
- Provider Dialog
- Workspace Dialog
- Context Viz

**Files:**
- Create: `rust/crates/icode-cli/src/tui/components/list_picker.rs` (generic helper)
- Create: `rust/crates/icode-cli/src/tui/components/mcp_dialog.rs`
- Create: `rust/crates/icode-cli/src/tui/components/skills_dialog.rs`
- Create: `rust/crates/icode-cli/src/tui/components/theme_list.rs`
- Create: `rust/crates/icode-cli/src/tui/components/plugins_dialog.rs`
- Create: `rust/crates/icode-cli/src/tui/components/prompt_stash.rs`
- Create: `rust/crates/icode-cli/src/tui/components/export_options.rs`
- Create: `rust/crates/icode-cli/src/tui/components/debug_panel.rs`
- Create: `rust/crates/icode-cli/src/tui/components/provider_dialog.rs`
- Create: `rust/crates/icode-cli/src/tui/components/workspace_dialog.rs`
- Create: `rust/crates/icode-cli/src/tui/components/context_viz.rs`
- Modify: `rust/crates/icode-cli/src/tui/components/mod.rs`
- Modify: `rust/crates/icode-cli/src/tui/app.rs`

- [ ] **Step 1: Create the generic ListPicker helper**

Create `rust/crates/icode-cli/src/tui/components/list_picker.rs`:

```rust
use crate::tui::app::AppState;
use crate::tui::component::{Component, ComponentAction};
use crate::tui::keybinds::{KeyAction, KeybindRegistry};
use crate::tui::Theme;
use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;

/// A generic component for dialogs that share the same navigation pattern:
/// Esc=close, Up/Down=navigate, Enter=confirm, Char=type, Backspace=delete.
pub struct ListPicker<S, R> {
    pub state: S,
    /// Called when Enter is pressed. Returns an action.
    pub on_confirm: R,
    pub render_fn: fn(&S, &mut Frame, Rect, Theme),
    pub name: &'static str,
    pub is_picker: bool,
}

impl<S, R> ListPicker<S, R>
where
    S: CursorNav + TextInput,
    R: Fn(&mut S, &mut AppState) -> ComponentAction,
{
    pub fn new(
        mut state: S,
        on_confirm: R,
        render_fn: fn(&S, &mut Frame, Rect, Theme),
        name: &'static str,
    ) -> Self {
        state.open();
        Self {
            state,
            on_confirm,
            render_fn,
            name,
            is_picker: false,
        }
    }
}

impl<S, R> Component for ListPicker<S, R>
where
    S: CursorNav + TextInput,
    R: Fn(&mut S, &mut AppState) -> ComponentAction,
{
    fn handle_key(
        &mut self,
        key: KeyEvent,
        app: &mut AppState,
        registry: &KeybindRegistry,
    ) -> ComponentAction {
        if registry.matches(&KeyAction::DialogCancel, &key) {
            return ComponentAction::Close;
        }
        if registry.matches(&KeyAction::DialogConfirm, &key) {
            return (self.on_confirm)(&mut self.state, app);
        }
        if registry.matches(&KeyAction::DialogUp, &key) {
            self.state.cursor_up();
        }
        if registry.matches(&KeyAction::DialogDown, &key) {
            self.state.cursor_down();
        }
        if registry.matches(&KeyAction::DialogPageUp, &key) {
            self.state.page_up();
        }
        if registry.matches(&KeyAction::DialogPageDown, &key) {
            self.state.page_down();
        }
        if registry.matches(&KeyAction::DialogSearch, &key) {
            // Already in search mode for these dialogs — type_char('/')
            self.state.type_char('/');
        }
        if let KeyCode::Char(c) = key.code {
            self.state.type_char(c);
        }
        if matches!(key.code, KeyCode::Backspace) {
            self.state.backspace();
        }
        ComponentAction::None
    }

    fn render(&self, frame: &mut Frame, area: Rect, theme: Theme) {
        (self.render_fn)(&self.state, frame, area, theme);
    }

    fn is_picker(&self) -> bool { self.is_picker }

    fn name(&self) -> &'static str { self.name }
}

/// Trait for state structs that support cursor navigation.
pub trait CursorNav {
    fn open(&mut self);
    fn cursor_up(&mut self);
    fn cursor_down(&mut self);
    fn page_up(&mut self) { /* no-op by default */ }
    fn page_down(&mut self) { /* no-op by default */ }
}

/// Trait for state structs that support text input.
pub trait TextInput {
    fn type_char(&mut self, c: char);
    fn backspace(&mut self);
}
```

- [ ] **Step 2: Implement CursorNav + TextInput for each dialog state**

In each dialog's existing file, add the trait impl. Example for `dialog_mcp.rs`:

Add to `rust/crates/icode-cli/src/tui/dialog_mcp.rs`:

```rust
use crate::tui::components::list_picker::{CursorNav, TextInput};

impl CursorNav for McpDialogState {
    fn open(&mut self) { self.open(); }
    fn cursor_up(&mut self) { self.cursor_up(); }
    fn cursor_down(&mut self) { self.cursor_down(); }
}

impl TextInput for McpDialogState {
    fn type_char(&mut self, c: char) { self.type_char(c); }
    fn backspace(&mut self) { self.backspace(); }
}
```

Do this for: `McpDialogState`, `SkillsDialogState`, `ThemeListDialogState`, `PluginsDialogState`, `PromptStashState`, `ExportOptionsState`, `DebugPanelState`, `ProviderDialogState`, `WorkspaceDialogState`, `ContextVizDialogState`.

Each needs `CursorNav` and `TextInput` impls. Some dialogs may need `page_up`/`page_down` overrides.

- [ ] **Step 3: Create each Component wrapper**

Example for MCP — create `rust/crates/icode-cli/src/tui/components/mcp_dialog.rs`:

```rust
use crate::tui::app::AppState;
use crate::tui::component::ComponentAction;
use crate::tui::components::list_picker::ListPicker;
use crate::tui::dialog_mcp::McpDialogState;
use crate::tui::dialog_mcp::render_mcp_dialog;

pub type McpDialogComponent = ListPicker<McpDialogState, fn(&mut McpDialogState, &mut AppState) -> ComponentAction>;

impl McpDialogComponent {
    pub fn new() -> Self {
        ListPicker::new(
            McpDialogState::new(),
            |state: &mut McpDialogState, _app: &mut AppState| {
                state.toggle_server();
                ComponentAction::None
            },
            render_mcp_dialog,
            "McpDialog",
        )
    }
}
```

Repeat this pattern for all 10 dialogs. Each one:
1. Create a `pub type XyzComponent = ListPicker<XyzState, fn(...)>;`
2. Implement `new()` constructor
3. The `on_confirm` closure calls the dialog-specific action (toggle_server, select_theme, etc.)

- [ ] **Step 4: Register all in components/mod.rs**

```rust
pub mod list_picker;
pub mod help_dialog;
pub mod mcp_dialog;
pub mod skills_dialog;
pub mod theme_list;
pub mod plugins_dialog;
pub mod prompt_stash;
pub mod export_options;
pub mod debug_panel;
pub mod provider_dialog;
pub mod workspace_dialog;
pub mod context_viz;

pub use help_dialog::HelpDialogComponent;
pub use mcp_dialog::McpDialogComponent;
pub use skills_dialog::SkillsDialogComponent;
pub use theme_list::ThemeListComponent;
pub use plugins_dialog::PluginsDialogComponent;
pub use prompt_stash::PromptStashComponent;
pub use export_options::ExportOptionsComponent;
pub use debug_panel::DebugPanelComponent;
pub use provider_dialog::ProviderDialogComponent;
pub use workspace_dialog::WorkspaceDialogComponent;
pub use context_viz::ContextVizComponent;
```

- [ ] **Step 5: Update AppState open_* methods**

Replace each `open_*` method to use the Component pattern. Example:

```rust
pub fn open_mcp(&mut self) {
    self.mcp_dialog.open = true;
    self.modal_stack.push(Box::new(McpDialogComponent::new()), self);
}
```

Do this for all 10 dialogs.

- [ ] **Step 6: Verify compilation**

Run: `cargo check -p icode-cli 2>&1 | head -60`
Expected: Should compile. Fix any trait impl issues.

- [ ] **Step 7: Commit**

```bash
git add rust/crates/icode-cli/src/tui/components/ rust/crates/icode-cli/src/tui/app.rs
git commit -m "refactor(tui): migrate 10 list-picker dialogs to Component pattern"
```

---

### Task 5: Migrate complex dialogs (ModelPicker, CommandPalette, Sessions, Permission, Question)

These dialogs have unique key handling that doesn't fit the ListPicker pattern.

**Files:**
- Create: `rust/crates/icode-cli/src/tui/components/model_picker.rs`
- Create: `rust/crates/icode-cli/src/tui/components/command_palette.rs`
- Create: `rust/crates/icode-cli/src/tui/components/sessions_dialog.rs`
- Create: `rust/crates/icode-cli/src/tui/components/message_action.rs`
- Create: `rust/crates/icode-cli/src/tui/components/session_branching.rs`
- Modify: `rust/crates/icode-cli/src/tui/components/mod.rs`
- Modify: `rust/crates/icode-cli/src/tui/app.rs`

- [ ] **Step 1: ModelPicker Component**

Create `rust/crates/icode-cli/src/tui/components/model_picker.rs`:

```rust
use crate::tui::app::{AppState, ToastKind};
use crate::tui::component::{Component, ComponentAction};
use crate::tui::dialog_mcp::render_mcp_dialog;
use crate::tui::keybinds::{KeyAction, KeybindRegistry};
use crate::tui::model_picker::ModelPickerState;
use crate::tui::model_picker::render_model_picker;
use crate::tui::Theme;
use crossterm::event::{KeyEvent, KeyCode, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;

pub struct ModelPickerComponent {
    state: ModelPickerState,
}

impl ModelPickerComponent {
    pub fn new(mut state: ModelPickerState) -> Self {
        state.open();
        Self { state }
    }
}

impl Component for ModelPickerComponent {
    fn handle_key(
        &mut self,
        key: KeyEvent,
        app: &mut AppState,
        registry: &KeybindRegistry,
    ) -> ComponentAction {
        if registry.matches(&KeyAction::DialogCancel, &key) {
            return ComponentAction::Close;
        }
        if registry.matches(&KeyAction::DialogConfirm, &key) {
            self.state.confirm();
            if let Some(model) = self.state.selected.take() {
                app.session.model.clone_from(&model);
                app.add_toast(format!("Model changed to {model}"), ToastKind::Info);
            }
            return ComponentAction::CloseWithValue("/model".to_string());
        }
        if registry.matches(&KeyAction::ModelPickerFavorite, &key) {
            self.state.toggle_favorite();
        }
        if registry.matches(&KeyAction::DialogSearch, &key) {
            self.state.type_char('/');
        }
        if registry.matches(&KeyAction::DialogUp, &key) {
            self.state.cursor_up();
        }
        if registry.matches(&KeyAction::DialogDown, &key) {
            self.state.cursor_down();
        }
        if let KeyCode::Char(c) = key.code {
            self.state.type_char(c);
        }
        if matches!(key.code, KeyCode::Backspace) {
            self.state.backspace();
        }
        ComponentAction::None
    }

    fn render(&self, frame: &mut Frame, area: Rect, theme: Theme) {
        render_model_picker(frame, &self.state, area, theme);
    }

    fn is_picker(&self) -> bool { true }

    fn name(&self) -> &'static str { "ModelPicker" }
}
```

- [ ] **Step 2: CommandPalette Component**

Create `rust/crates/icode-cli/src/tui/components/command_palette.rs`:

```rust
use crate::tui::app::AppState;
use crate::tui::command_palette::CommandPaletteState;
use crate::tui::command_palette::render_command_palette;
use crate::tui::command_palette::CommandAction;
use crate::tui::component::{Component, ComponentAction};
use crate::tui::keybinds::{KeyAction as KA, KeybindRegistry};
use crate::tui::Theme;
use crossterm::event::{KeyEvent, KeyCode};
use ratatui::Frame;
use ratatui::layout::Rect;

pub struct CommandPaletteComponent {
    state: CommandPaletteState,
}

impl CommandPaletteComponent {
    pub fn new(mut state: CommandPaletteState) -> Self {
        state.open();
        Self { state }
    }
}

impl Component for CommandPaletteComponent {
    fn handle_key(
        &mut self,
        key: KeyEvent,
        _app: &mut AppState,
        registry: &KeybindRegistry,
    ) -> ComponentAction {
        if registry.matches(&KA::DialogCancel, &key) {
            return ComponentAction::Close;
        }
        if registry.matches(&KA::DialogConfirm, &key) {
            let action = self.state.filtered.get(self.state.cursor)
                .and_then(|&entry_idx| self.state.entries.get(entry_idx).map(|e| e.action.clone()));
            self.state.confirm();
            // The action dispatch will be handled by the caller
            return ComponentAction::Close;
        }
        if registry.matches(&KA::DialogSearch, &key) {
            self.state.type_char('/');
        }
        if registry.matches(&KA::DialogUp, &key) {
            self.state.cursor_up();
        }
        if registry.matches(&KA::DialogDown, &key) {
            self.state.cursor_down();
        }
        if let KeyCode::Char(c) = key.code {
            self.state.type_char(c);
        }
        if matches!(key.code, KeyCode::Backspace) {
            self.state.backspace();
        }
        ComponentAction::None
    }

    fn render(&self, frame: &mut Frame, area: Rect, theme: Theme) {
        render_command_palette(frame, &self.state, area, theme);
    }

    fn is_picker(&self) -> bool { true }

    fn name(&self) -> &'static str { "CommandPalette" }
}
```

- [ ] **Step 3: Sessions, MessageAction, SessionBranching Components**

Create the remaining complex components following the same pattern. Each one:
1. Wraps the existing `*State` struct
2. Implements `handle_key` with its unique logic
3. Delegates `render` to the existing `render_*` function
4. Returns appropriate `ComponentAction`

For `SessionsDialogComponent`: handle session selection, deletion, etc.
For `MessageActionComponent`: handle r/c/f key actions (use `registry.matches_key_code`).
For `SessionBranchingComponent`: handle branch creation/selection.

- [ ] **Step 4: Update components/mod.rs**

Add exports for all new components.

- [ ] **Step 5: Update AppState open_* methods**

Replace remaining `open_*` methods:
```rust
pub fn open_model_picker(&mut self) {
    self.model_picker.open();
    self.modal_stack.push(Box::new(ModelPickerComponent::new(
        std::mem::take(&mut self.model_picker),
    )), self);
}

pub fn open_command_palette(&mut self) {
    self.command_palette.open();
    self.modal_stack.push(Box::new(CommandPaletteComponent::new(
        std::mem::take(&mut self.command_palette),
    )), self);
}
```

- [ ] **Step 6: Verify compilation**

Run: `cargo check -p icode-cli 2>&1 | head -60`

- [ ] **Step 7: Commit**

```bash
git add rust/crates/icode-cli/src/tui/components/ rust/crates/icode-cli/src/tui/app.rs
git commit -m "refactor(tui): migrate complex dialogs to Component pattern"
```

---

### Task 6: Migrate remaining dialogs (DiffView, Pager, Autocomplete) + Permission/Question

**Files:**
- Create: `rust/crates/icode-cli/src/tui/components/diff_view.rs`
- Create: `rust/crates/icode-cli/src/tui/components/pager.rs`
- Create: `rust/crates/icode-cli/src/tui/components/autocomplete.rs`
- Create: `rust/crates/icode-cli/src/tui/components/permission.rs`
- Create: `rust/crates/icode-cli/src/tui/components/question.rs`
- Modify: `rust/crates/icode-cli/src/tui/components/mod.rs`
- Modify: `rust/crates/icode-cli/src/tui/app.rs`

- [ ] **Step 1: DiffView Component**

Create `rust/crates/icode-cli/src/tui/components/diff_view.rs`:

```rust
use crate::tui::app::AppState;
use crate::tui::component::{Component, ComponentAction};
use crate::tui::keybinds::{KeyAction, KeybindRegistry};
use crate::tui::widgets::DiffView;
use crate::tui::widgets::render_diff_view_overlay;
use crate::tui::Theme;
use crossterm::event::{KeyEvent, KeyCode};
use ratatui::Frame;
use ratatui::layout::Rect;

pub struct DiffViewComponent {
    view: DiffView,
}

impl DiffViewComponent {
    pub fn new(view: DiffView) -> Self { Self { view } }
}

impl Component for DiffViewComponent {
    fn handle_key(
        &mut self,
        key: KeyEvent,
        _app: &mut AppState,
        registry: &KeybindRegistry,
    ) -> ComponentAction {
        if registry.matches(&KeyAction::DialogCancel, &key) {
            return ComponentAction::Close;
        }
        if registry.matches(&KeyAction::DialogPageUp, &key) {
            self.view.scroll_up();
        }
        if registry.matches(&KeyAction::DialogPageDown, &key) {
            self.view.scroll_down();
        }
        if registry.matches(&KeyAction::DialogUp, &key) {
            self.view.scroll_line_up();
        }
        if registry.matches(&KeyAction::DialogDown, &key) {
            self.view.scroll_line_down();
        }
        ComponentAction::None
    }

    fn render(&self, frame: &mut Frame, area: Rect, theme: Theme) {
        render_diff_view_overlay(frame, &self.view, area, &theme);
    }

    fn name(&self) -> &'static str { "DiffView" }
}
```

- [ ] **Step 2: Pager Component**

Create `rust/crates/icode-cli/src/tui/components/pager.rs`:

```rust
use crate::tui::app::AppState;
use crate::tui::component::{Component, ComponentAction};
use crate::tui::keybinds::{KeyAction, KeybindRegistry};
use crate::tui::widgets::{PagerState, render_pager};
use crate::tui::Theme;
use crossterm::event::{KeyEvent, KeyCode};
use ratatui::Frame;
use ratatui::layout::Rect;

pub struct PagerComponent {
    state: PagerState,
}

impl PagerComponent {
    pub fn new(title: String, content: String) -> Self {
        let mut state = PagerState::default();
        state.open(title, content);
        Self { state }
    }
}

impl Component for PagerComponent {
    fn handle_key(
        &mut self,
        key: KeyEvent,
        _app: &mut AppState,
        registry: &KeybindRegistry,
    ) -> ComponentAction {
        if registry.matches(&KeyAction::DialogCancel, &key) {
            return ComponentAction::Close;
        }
        if registry.matches(&KeyAction::DialogPageUp, &key) {
            self.state.page_up();
        }
        if registry.matches(&KeyAction::DialogPageDown, &key) {
            self.state.page_down();
        }
        if registry.matches(&KeyAction::DialogUp, &key) {
            self.state.scroll_up();
        }
        if registry.matches(&KeyAction::DialogDown, &key) {
            self.state.scroll_down();
        }
        ComponentAction::None
    }

    fn render(&self, frame: &mut Frame, area: Rect, theme: Theme) {
        render_pager(frame, &self.state, area, &theme);
    }

    fn name(&self) -> &'static str { "Pager" }
}
```

- [ ] **Step 3: Autocomplete Component**

Create `rust/crates/icode-cli/src/tui/components/autocomplete.rs`:

```rust
use crate::tui::app::AppState;
use crate::tui::autocomplete::AutocompleteState;
use crate::tui::autocomplete::render_autocomplete_overlay;
use crate::tui::component::{Component, ComponentAction};
use crate::tui::Theme;
use crossterm::event::{KeyEvent, KeyCode};
use ratatui::Frame;
use ratatui::layout::Rect;

pub struct AutocompleteComponent {
    state: AutocompleteState,
}

impl AutocompleteComponent {
    pub fn new() -> Self {
        Self { state: AutocompleteState::new() }
    }
}

impl Component for AutocompleteComponent {
    fn handle_key(
        &mut self,
        key: KeyEvent,
        app: &mut AppState,
        _registry: &KeybindRegistry,
    ) -> ComponentAction {
        match key.code {
            KeyCode::Esc => {
                self.state.close();
                return ComponentAction::Close;
            }
            KeyCode::Enter => {
                self.state.select(&mut app.prompt);
                if let Some(ref mut frecency) = app.prompt.frecency {
                    if let Some(entry) = self.state.entries.get(self.state.idx) {
                        frecency.record(&entry.title);
                    }
                }
                return ComponentAction::Close;
            }
            KeyCode::Up => self.state.cursor_up(),
            KeyCode::Down => self.state.cursor_down(),
            _ => {}
        }
        ComponentAction::None
    }

    fn render(&self, frame: &mut Frame, area: Rect, theme: Theme) {
        render_autocomplete_overlay(frame, &self.state, area, theme);
    }

    fn name(&self) -> &'static str { "Autocomplete" }
}
```

- [ ] **Step 4: Permission Component**

Create `rust/crates/icode-cli/src/tui/components/permission.rs`:

```rust
use crate::tui::app::AppState;
use crate::tui::component::{Component, ComponentAction};
use crate::tui::dialog_permission::PermissionDialogState;
use crate::tui::dialog_permission::render_permission_dialog;
use crate::tui::dialog_permission::PermissionAction;
use crate::tui::Theme;
use crossterm::event::{KeyEvent, KeyCode};
use ratatui::Frame;
use ratatui::layout::Rect;
use runtime::{PermissionPromptDecision, PermissionResponseTx};

pub struct PermissionComponent {
    state: PermissionDialogState,
    tx: Option<PermissionResponseTx>,
}

impl PermissionComponent {
    pub fn new(state: PermissionDialogState, tx: PermissionResponseTx) -> Self {
        Self { state, tx: Some(tx) }
    }
}

impl Component for PermissionComponent {
    fn handle_key(
        &mut self,
        key: KeyEvent,
        _app: &mut AppState,
        _registry: &KeybindRegistry,
    ) -> ComponentAction {
        if let Some(action) = self.state.handle_key(key.code) {
            let decision = match action {
                PermissionAction::Approve | PermissionAction::AlwaysAllow => {
                    PermissionPromptDecision::Allow
                }
                PermissionAction::Deny => PermissionPromptDecision::Deny {
                    reason: "denied by user in TUI".into(),
                },
            };
            if let Some(tx) = self.tx.take() {
                let _ = tx.send(decision);
            }
            return ComponentAction::Close;
        }
        if matches!(key.code, KeyCode::Esc) {
            if let Some(tx) = self.tx.take() {
                let _ = tx.send(PermissionPromptDecision::Deny {
                    reason: "dismissed by user".into(),
                });
            }
            return ComponentAction::Close;
        }
        ComponentAction::None
    }

    fn render(&self, frame: &mut Frame, area: Rect, theme: Theme) {
        render_permission_dialog(frame, &self.state, area, theme);
    }

    fn is_blocking(&self) -> bool { true }

    fn name(&self) -> &'static str { "Permission" }
}
```

- [ ] **Step 5: Question Component**

Create `rust/crates/icode-cli/src/tui/components/question.rs`:

```rust
use crate::tui::app::AppState;
use crate::tui::component::{Component, ComponentAction};
use crate::tui::dialog_question::QuestionPromptState;
use crate::tui::dialog_question::render_question_prompt;
use crate::tui::Theme;
use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;

pub struct QuestionComponent {
    state: QuestionPromptState,
}

impl QuestionComponent {
    pub fn new(state: QuestionPromptState) -> Self { Self { state } }
}

impl Component for QuestionComponent {
    fn handle_key(
        &mut self,
        _key: KeyEvent,
        _app: &mut AppState,
        _registry: &KeybindRegistry,
    ) -> ComponentAction {
        // Question prompt has its own internal key handling via state.handle_key
        if let Some(response) = self.state.handle_key(_key) {
            if let Some(tx) = self.state.answer_tx.take() {
                let _ = tx.send(response.answer.clone());
            }
            return ComponentAction::Close;
        }
        ComponentAction::None
    }

    fn render(&self, frame: &mut Frame, area: Rect, theme: Theme) {
        render_question_prompt(frame, area, &self.state, &theme);
    }

    fn is_blocking(&self) -> bool { true }

    fn name(&self) -> &'static str { "Question" }
}
```

- [ ] **Step 6: Update AppState open_* methods**

Replace all remaining `open_*` methods to use Component pattern.

- [ ] **Step 7: Verify compilation**

Run: `cargo check -p icode-cli 2>&1 | head -60`

- [ ] **Step 8: Commit**

```bash
git add rust/crates/icode-cli/src/tui/components/ rust/crates/icode-cli/src/tui/app.rs
git commit -m "refactor(tui): migrate remaining dialogs to Component pattern"
```

---

### Task 7: Delete ActiveModal enum + handle_*_from_modal methods

**Files:**
- Modify: `rust/crates/icode-cli/src/tui/runner.rs`
- Modify: `rust/crates/icode-cli/src/tui/layout.rs`
- Modify: `rust/crates/icode-cli/src/tui/modal_manager.rs`
- Modify: `rust/crates/icode-cli/src/tui/app.rs`

- [ ] **Step 1: Delete ActiveModal enum from modal_manager.rs**

Replace the contents of `rust/crates/icode-cli/src/tui/modal_manager.rs` with:

```rust
// ActiveModal has been replaced by the Component trait + ModalStack.
// This file is kept for backward compatibility during migration.
// All dialog state now uses Component implementations in tui/components/.
//
// Delete this file once all callers are migrated.
```

Actually, after all dialogs are migrated, delete the file entirely:

Remove from `mod.rs`:
```rust
pub mod modal_manager;
pub use modal_manager::ActiveModal;
```

- [ ] **Step 2: Delete handle_*_from_modal methods from runner.rs**

Delete ALL these methods from `runner.rs`:
- `handle_permission_action_from_modal` (line ~1297)
- `handle_question_action_from_modal` (line ~1331)
- `handle_picker_action_from_modal` (line ~1345)
- `handle_palette_action_from_modal` (line ~1495)
- `handle_mcp_action_from_modal` (line ~1542)
- `handle_skills_action_from_modal` (line ~1580)
- `handle_theme_list_action_from_modal` (line ~1614)
- `handle_plugins_action_from_modal` (line ~1656)
- `handle_sessions_action_from_modal` (line ~1694)
- `handle_message_action_action_from_modal` (line ~1714)
- `handle_help_action_from_modal` (line ~1750)
- `handle_context_viz_action_from_modal` (line ~1764)
- `handle_branching_action_from_modal` (line ~1778)
- `handle_stash_action_from_modal` (line ~1801)
- `handle_export_options_action_from_modal` (line ~1841)
- `handle_debug_panel_action_from_modal` (line ~1865)
- `handle_provider_action_from_modal` (line ~1882)
- `handle_workspace_action_from_modal` (line ~1960)
- `handle_diff_view_action_from_modal` (line ~1994)
- `handle_pager_action_from_modal` (line ~2037)
- `handle_autocomplete_action_from_modal` (line ~2081)

This should delete ~900 lines from runner.rs.

- [ ] **Step 3: Replace handle_key modal dispatch**

In `runner.rs` `handle_key()`, replace the ActiveModal match block (lines ~244-279):

```rust
let modal = self.state.active_modal.take();
if let Some(mut modal) = modal {
    let result = match &mut modal {
        ActiveModal::Permission(s) => self.handle_permission_action_from_modal(key, s),
        // ... 20 more lines ...
    };
    // ...
    return result;
}
```

With:
```rust
// Component stack handles all modal key events
if let Some(result) = self.state.modal_stack.handle_key(key, &mut self.state, &self.keybinds) {
    return Some(result);
}
```

- [ ] **Step 4: Update layout.rs render dispatch**

In `rust/crates/icode-cli/src/tui/layout.rs`, replace the `ActiveModal` match block (lines ~134-207):

```rust
if let Some(ref modal) = state.active_modal {
    match modal {
        ActiveModal::Permission(s) => render_permission_dialog(frame, s, area, theme),
        // ... 20 more lines ...
    }
}
```

With:
```rust
use crate::tui::component::ModalStack;

// Render all active modal components
state.modal_stack.render(frame, area, theme);
```

Also remove the `use crate::tui::modal_manager::ActiveModal;` import.

- [ ] **Step 5: Clean up AppState**

In `rust/crates/icode-cli/src/tui/app.rs`:
- Remove `pub active_modal: Option<ActiveModal>` field (replaced by `modal_stack`)
- Remove all `ActiveModal::Xxx` imports
- Keep the `open_permission` / `open_question` methods — they now push components

- [ ] **Step 6: Verify compilation**

Run: `cargo check -p icode-cli 2>&1 | head -60`
Expected: Clean compilation.

Run: `cargo build -p icode-cli 2>&1 | tail -5`
Expected: Successful build.

- [ ] **Step 7: Run clippy**

Run: `cargo clippy -p icode-cli -- -D warnings 2>&1 | head -40`
Expected: Clean (or only pre-existing warnings).

- [ ] **Step 8: Run tests**

Run: `cargo test -p icode-cli 2>&1 | tail -20`
Expected: All existing tests pass.

- [ ] **Step 9: Commit**

```bash
git add rust/crates/icode-cli/src/tui/runner.rs rust/crates/icode-cli/src/tui/layout.rs rust/crates/icode-cli/src/tui/modal_manager.rs rust/crates/icode-cli/src/tui/app.rs
git commit -m "refactor(tui): remove ActiveModal enum and 21 handle_*_from_modal methods

Replaced with Component trait + ModalStack pattern.
- Deleted ~900 lines of copy-paste modal handlers
- Deleted ActiveModal enum (21 variants)
- All dialogs now implement Component trait
- KeybindRegistry wired into dispatch layer"
```

---

### Task 8: Full KeybindRegistry integration

**Files:**
- Modify: `rust/crates/icode-cli/src/tui/runner.rs`
- Modify: `rust/crates/icode-cli/src/tui/app.rs`
- Modify: `rust/crates/icode-cli/src/tui/keybinds.rs`

- [ ] **Step 1: Wire keybinds into normal (non-modal) key handling**

In `runner.rs` `handle_key()`, replace hardcoded matches with registry lookups.

Current:
```rust
match (key.modifiers, key.code) {
    (KeyModifiers::CONTROL, KeyCode::Char('c')) => { ... }
    (KeyModifiers::CONTROL, KeyCode::Char('m')) => { ... }
    (KeyModifiers::CONTROL, KeyCode::Char('p')) => { ... }
    (KeyModifiers::CONTROL, KeyCode::Char('x')) => { ... }
    // ... 50+ more lines ...
}
```

Replace with:
```rust
// Check global actions via registry
if self.keybinds.matches(&KeyAction::Quit, &key) {
    if self.state.is_streaming {
        self.state.mode = AppMode::Normal;
        self.state.is_streaming = false;
        self.state.finish_stream();
        None
    } else {
        Some(String::new())
    }
} else if self.keybinds.matches(&KeyAction::ModelPicker, &key) {
    self.state.open_model_picker();
    None
} else if self.keybinds.matches(&KeyAction::CommandPalette, &key) {
    self.state.open_command_palette();
    None
} else if self.keybinds.matches(&KeyAction::Help, &key) {
    self.state.open_help();
    None
} else if self.keybinds.matches(&KeyAction::ScrollPageUp, &key) {
    self.state.scroll_messages_up();
    None
} else if self.keybinds.matches(&KeyAction::ScrollPageDown, &key) {
    self.state.scroll_messages_down();
    None
} else if self.keybinds.matches(&KeyAction::ToggleSidebar, &key) {
    self.state.sidebar_visible = !self.state.sidebar_visible;
    None
} else if self.keybinds.matches(&KeyAction::ToggleDetails, &key) {
    self.state.toggle_details();
    None
} else {
    // Fall through to leader key, autocomplete, and text input
    // ... existing code for leader key, Enter, Tab, char input, etc.
}
```

- [ ] **Step 2: Add config override support**

In `Tui::new()`, load keybind overrides from config:

```rust
let mut keybinds = KeybindRegistry::new();
keybinds.populate_defaults();

// Load overrides from config
if let Some(kv) = state.config.get("keybinds") {
    if let Ok(overrides) = serde_json::from_str::<HashMap<String, String>>(kv) {
        keybinds.apply_overrides(&overrides);
    }
}
```

- [ ] **Step 3: Add KeyAction variants for missing actions**

The current `KeyAction` enum has 25 variants. Add any missing ones that correspond to hardcoded matches in `handle_key()`:

```rust
// Add to KeyAction enum:
RefreshScreen,
NewlineInPrompt,
// ... any others from the current hardcoded matches ...
```

Update `populate_defaults()` and `parse_action()` accordingly.

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p icode-cli 2>&1 | head -40`

- [ ] **Step 5: Test keybind functionality**

Run: `cargo test -p icode-cli 2>&1 | tail -20`

- [ ] **Step 6: Commit**

```bash
git add rust/crates/icode-cli/src/tui/runner.rs rust/crates/icode-cli/src/tui/keybinds.rs rust/crates/icode-cli/src/tui/app.rs
git commit -m "feat(tui): wire KeybindRegistry into all key handling paths"
```

---

### Task 9: Extract runner.rs into focused modules

**Files:**
- Create: `rust/crates/icode-cli/src/tui/runner/terminal.rs` (terminal setup/teardown)
- Create: `rust/crates/icode-cli/src/tui/runner/turn_events.rs` (poll_turn_events + process_turn_event)
- Create: `rust/crates/icode-cli/src/tui/runner/slash_commands.rs` (execute_slash_command)
- Create: `rust/crates/icode-cli/src/tui/runner/editor.rs` (open_external_editor)
- Create: `rust/crates/icode-cli/src/tui/runner/undo_redo.rs` (handle_undo/handle_redo)
- Modify: `rust/crates/icode-cli/src/tui/runner.rs` (becomes thin orchestrator)

- [ ] **Step 1: Extract terminal management**

Create `rust/crates/icode-cli/src/tui/runner/terminal.rs`:

```rust
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io;

use crate::tui::kitty::KittyKeyboard;

pub fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let _ = KittyKeyboard::enable();
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

pub fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
    )?;
    terminal.show_cursor()?;
    Ok(())
}
```

- [ ] **Step 2: Extract turn event handling**

Create `rust/crates/icode-cli/src/tui/runner/turn_events.rs`:

Move `poll_turn_events()` and `process_turn_event()` from runner.rs to this file.
Keep them as methods on Tui (or make them free functions taking `&mut Tui`).

- [ ] **Step 3: Extract slash command execution**

Create `rust/crates/icode-cli/src/tui/runner/slash_commands.rs`:

Move `execute_slash_command()` and related helpers to this file.

- [ ] **Step 4: Thin out runner.rs**

After extracting, runner.rs should be ~200 lines:
```rust
mod terminal;
mod turn_events;
mod slash_commands;
mod editor;
mod undo_redo;

pub struct Tui { ... }

impl Tui {
    pub fn new(...) -> Result<Self> { ... }

    pub fn run(&mut self) -> Result<String> {
        loop {
            // draw
            // event dispatch → keybinds → modal_stack → normal handling
            // tick → turn events
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Option<String> {
        // KeybindRegistry dispatch
        // ModalStack dispatch
        // Leader key
        // Text input
    }
}
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p icode-cli 2>&1 | head -40`
Run: `cargo build -p icode-cli 2>&1 | tail -5`
Run: `cargo test -p icode-cli 2>&1 | tail -20`

- [ ] **Step 6: Commit**

```bash
git add rust/crates/icode-cli/src/tui/runner.rs rust/crates/icode-cli/src/tui/runner/
git commit -m "refactor(tui): extract runner.rs into focused modules"
```

---

### Task 10: Final cleanup, linting, and verification

**Files:**
- All modified TUI files

- [ ] **Step 1: Remove unused imports**

Run: `cargo clippy -p icode-cli -- -D warnings 2>&1`
Fix all warnings about unused imports, dead code, etc.

- [ ] **Step 2: Remove dialog `open` field redundancy**

Many dialog states have `pub open: bool` fields that are now redundant (the ModalStack tracks open/closed). Keep them for backward compatibility but add `#[allow(dead_code)]` or document their deprecation.

- [ ] **Step 3: Format**

Run: `cargo fmt -p icode-cli`

- [ ] **Step 4: Full build + test**

Run:
```bash
cargo fmt -p icode-cli
cargo clippy -p icode-cli -- -D warnings
cargo test -p icode-cli
cargo build -p icode-cli --release
```

All must pass cleanly.

- [ ] **Step 5: Measure impact**

Run:
```bash
wc -l rust/crates/icode-cli/src/tui/runner.rs
wc -l rust/crates/icode-cli/src/tui/modal_manager.rs
echo "---"
find rust/crates/icode-cli/src/tui/components/ -name '*.rs' -exec wc -l {} +
```

Expected: runner.rs dropped from ~3993 to ~600 lines, modal_manager.rs deleted, components/ directory ~800-1200 lines total.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "chore(tui): final cleanup, formatting, and linting"
```

---

## Self-Review

### 1. Spec Coverage

| Requirement | Task |
|---|---|
| Component trait with handle_key/render/on_mount/on_unmount | Task 1 |
| ModalStack replacing ActiveModal enum | Tasks 1, 2, 7 |
| Eliminate 21 copy-paste handle_*_from_modal methods | Tasks 3-7 |
| Wire up existing KeybindRegistry | Tasks 3, 4, 8 |
| Generic ListPicker for shared dialog pattern | Task 4 |
| Each existing dialog becomes a Component impl | Tasks 3-6 |
| Keep all existing functionality intact | All tasks (gradual migration) |
| Split runner.rs into focused modules | Task 9 |
| No new dependencies | All tasks (same deps) |

### 2. Placeholder Scan
- No TBD/TODO patterns in task steps (except intentional transition comments)
- All code steps contain actual code
- All test commands are specific with expected output
- No "similar to Task N" references

### 3. Type Consistency
- `ComponentAction` enum used consistently across all component impls
- `KeybindRegistry` methods (`matches`, `matches_key_code`) used correctly
- `ModalStack` API (`push`, `handle_key`, `render`, `clear`, `is_blocking`) consistent
- All `open_*` methods in AppState push `Box<dyn Component>` — no mixing with ActiveModal after Task 7
- `KeyAction` variants match between keybinds.rs and component handle_key implementations

---

Plan complete. Two execution options:

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

Which approach?
