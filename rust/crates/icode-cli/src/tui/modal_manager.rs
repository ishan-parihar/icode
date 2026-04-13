use crate::tui::autocomplete::AutocompleteState;
use crate::tui::command_palette::CommandPaletteState;
use crate::tui::debug_panel::DebugPanelState;
use crate::tui::dialog_context_viz::ContextVizDialogState;
use crate::tui::dialog_export_options::ExportOptionsState;
use crate::tui::dialog_help::HelpDialogState;
use crate::tui::dialog_mcp::McpDialogState;
use crate::tui::dialog_message_actions::MessageActionDialogState;
use crate::tui::dialog_permission::PermissionDialogState;
use crate::tui::dialog_plugins::PluginsDialogState;
use crate::tui::dialog_prompt_stash::PromptStashState;
use crate::tui::dialog_providers::ProviderDialogState;
use crate::tui::dialog_question::QuestionPromptState;
use crate::tui::dialog_session_branching::SessionBranchingState;
use crate::tui::dialog_sessions::SessionsDialogState;
use crate::tui::dialog_skills::SkillsDialogState;
use crate::tui::dialog_theme_list::ThemeListDialogState;
use crate::tui::dialog_workspaces::WorkspaceDialogState;
use crate::tui::model_picker::ModelPickerState;
use crate::tui::widgets::{DiffView, PagerState};

pub enum ActiveModal {
    Permission(PermissionDialogState),
    Question(QuestionPromptState),
    ModelPicker(ModelPickerState),
    CommandPalette(CommandPaletteState),
    Mcp(McpDialogState),
    Skills(SkillsDialogState),
    ThemeList(ThemeListDialogState),
    Plugins(PluginsDialogState),
    Sessions(SessionsDialogState),
    MessageAction(MessageActionDialogState),
    Help(HelpDialogState),
    ContextViz(ContextVizDialogState),
    SessionBranching(SessionBranchingState),
    PromptStash(PromptStashState),
    ExportOptions(ExportOptionsState),
    DebugPanel(DebugPanelState),
    Provider(ProviderDialogState),
    Workspace(WorkspaceDialogState),
    DiffView(DiffView),
    Pager(PagerState),
    Autocomplete(AutocompleteState),
}

impl ActiveModal {
    pub fn is_blocking(&self) -> bool {
        matches!(self, ActiveModal::Permission(_) | ActiveModal::Question(_))
    }

    pub fn is_picker(&self) -> bool {
        matches!(
            self,
            ActiveModal::ModelPicker(_) | ActiveModal::CommandPalette(_)
        )
    }
}
