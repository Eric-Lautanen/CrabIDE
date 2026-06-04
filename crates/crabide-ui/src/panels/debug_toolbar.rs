//! Debug toolbar — shown inside the debug panel header when a session is active.
//!
//! Renders Continue / Step Over / Step In / Step Out / Restart / Stop buttons.
//! Returns a set of actions to execute.

use crabide_config::Action;

use crate::state::{cfg_to_egui, DapPanelState, UiState};

/// Render the debug toolbar.
///
/// Must be called inside a horizontal layout (`ui.horizontal`).
/// Returns a list of actions to forward to the app.
pub fn show(ui: &mut egui::Ui, state: &mut UiState) -> Vec<Action> {
    let mut actions: Vec<Action> = Vec::new();

    let dap = &mut state.dap_panel;
    if !dap.session_active {
        return actions;
    }

    let icon_size = egui::vec2(24.0, 22.0);
    let paused = dap.paused;

    // Continue / Pause
    if paused {
        if tool_btn(ui, "▶", "Continue (F5)", icon_size) {
            dap.pending_continue = true;
            actions.push(Action::ContinueDebug);
        }
    } else if tool_btn(ui, "⏸", "Pause", icon_size) {
        dap.pending_pause = true;
    }

    // Step Over
    let so_enabled = paused;
    if tool_btn_enabled(ui, "⤵", "Step Over (F10)", icon_size, so_enabled) {
        dap.pending_step_over = true;
        actions.push(Action::StepOver);
    }

    // Step Into
    if tool_btn_enabled(ui, "↓", "Step Into (F11)", icon_size, so_enabled) {
        dap.pending_step_in = true;
        actions.push(Action::StepInto);
    }

    // Step Out
    if tool_btn_enabled(ui, "↑", "Step Out (Shift+F11)", icon_size, so_enabled) {
        dap.pending_step_out = true;
        actions.push(Action::StepOut);
    }

    // Restart
    if tool_btn(ui, "🔃", "Restart", icon_size) {
        dap.pending_restart = true;
        actions.push(Action::RestartDebug);
    }

    // Stop
    if tool_btn(ui, "⏹", "Stop (Shift+F5)", icon_size) {
        dap.pending_stop = true;
        actions.push(Action::StopDebug);
    }

    actions
}

/// Render the launch config picker + start button (shown when no session is active).
pub fn show_launch_bar(ui: &mut egui::Ui, state: &mut UiState) -> Vec<Action> {
    let mut actions: Vec<Action> = Vec::new();
    let dap = &mut state.dap_panel;
    if dap.session_active {
        return actions;
    }

    let fg = cfg_to_egui(state.theme.ui_or(
        "editor.foreground",
        crabide_config::Color::rgb(0xcc, 0xcc, 0xcc),
    ));

    if dap.launch_configs.is_empty() {
        ui.label(
            egui::RichText::new("No launch configuration — add .crabide/launch.json")
                .color(egui::Color32::from_rgb(0x88, 0x88, 0x88))
                .size(12.0),
        );
    } else {
        // Config picker dropdown.
        let selected_name = dap
            .launch_configs
            .get(dap.selected_config_idx)
            .map(|c| c.name.as_str())
            .unwrap_or("(none)");
        egui::ComboBox::from_id_salt("dap_launch_picker")
            .width(200.0)
            .selected_text(egui::RichText::new(selected_name).color(fg).size(12.0))
            .show_ui(ui, |ui| {
                for (i, cfg) in dap.launch_configs.iter().enumerate() {
                    let selected = i == dap.selected_config_idx;
                    ui.selectable_value(
                        &mut dap.selected_config_idx,
                        i,
                        egui::RichText::new(&cfg.name).color(fg).size(12.0),
                    )
                    .changed();
                    let _ = selected;
                }
            });

        // Start button.
        let start_btn = egui::Button::new(
            egui::RichText::new("▶ Start Debugging (F5)")
                .color(egui::Color32::from_rgb(0x4e, 0xc9, 0xb0))
                .size(12.0),
        )
        .frame(false);

        if ui.add(start_btn).clicked() {
            dap.pending_launch = true;
            actions.push(Action::StartDebug);
        }
    }

    actions
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn tool_btn(ui: &mut egui::Ui, icon: &str, tooltip: &str, size: egui::Vec2) -> bool {
    tool_btn_enabled(ui, icon, tooltip, size, true)
}

fn tool_btn_enabled(
    ui: &mut egui::Ui,
    icon: &str,
    tooltip: &str,
    size: egui::Vec2,
    enabled: bool,
) -> bool {
    let alpha = if enabled { 0xff } else { 0x44 };
    let color = egui::Color32::from_rgba_unmultiplied(0xcc, 0xcc, 0xcc, alpha);

    let resp = ui.add_enabled(
        enabled,
        egui::Button::new(egui::RichText::new(icon).color(color).size(14.0))
            .min_size(size)
            .frame(false),
    );

    if !tooltip.is_empty() {
        resp.clone().on_hover_text(tooltip);
    }

    resp.clicked()
}

/// Render a compact "enable debugger" button for the status bar or nav area.
pub fn show_enable_btn(ui: &mut egui::Ui, dap: &mut DapPanelState) {
    let label = "🐛";
    let color = if dap.enabled {
        egui::Color32::from_rgb(0x4e, 0xc9, 0xb0)
    } else {
        egui::Color32::from_rgb(0x85, 0x85, 0x85)
    };
    let resp = ui.add(
        egui::Button::new(egui::RichText::new(label).color(color).size(11.0))
            .frame(false)
            .min_size(egui::vec2(28.0, 18.0)),
    );
    if resp.clicked() {
        dap.enabled = !dap.enabled;
        if !dap.enabled {
            // Disabling stops an active session.
            dap.pending_stop = true;
            dap.visible = false;
        }
    }
    let tooltip = if dap.enabled {
        "Debugger enabled — click to disable"
    } else {
        "Debugger disabled — click to enable"
    };
    resp.on_hover_text(tooltip);
}
