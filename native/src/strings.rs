pub mod global {
    pub fn btn_save_settings() -> String { crate::views::t("action.save_settings") }
    pub fn btn_revert_settings() -> String { crate::views::t("action.revert_settings") }
    pub fn btn_cancel() -> String { crate::views::t("action.cancel") }
    pub fn btn_dismiss() -> String { crate::views::t("action.dismiss") }
    pub fn btn_browse() -> String { crate::views::t("action.browse") }
    pub fn btn_remove() -> String { crate::views::t("action.remove") }
    pub fn btn_up() -> String { crate::views::t("action.up") }
    pub fn btn_down() -> String { crate::views::t("action.down") }
    pub fn btn_delete() -> String { crate::views::t("action.delete") }
    pub fn btn_remove_demo() -> String { crate::views::t("action.remove_demo") }
}

pub mod workspace {
    pub fn header_master_list() -> String { crate::views::t("workspace.header_master_list") }
    pub fn btn_add_demo_files() -> String { crate::views::t("workspace.btn_add_demo_files") }
    pub fn btn_save_global_settings() -> String { crate::views::t("workspace.btn_save_global_settings") }
    pub fn btn_reset_to_defaults() -> String { crate::views::t("workspace.btn_reset_to_defaults") }
    pub fn lbl_discovered_highlights() -> String { crate::views::t("workspace.lbl_discovered_highlights") }
    pub fn btn_select_all() -> String { crate::views::t("workspace.btn_select_all") }
    pub fn btn_deselect_all() -> String { crate::views::t("workspace.btn_deselect_all") }
    pub fn btn_preview() -> String { crate::views::t("workspace.btn_preview") }
    pub fn btn_add_drive() -> String { crate::views::t("workspace.btn_add_drive") }
    pub fn btn_force_relaunch() -> String { crate::views::t("workspace.btn_force_relaunch") }
    pub fn btn_copy_view_command() -> String { crate::views::t("workspace.btn_copy_view_command") }
}

pub mod capture {
    pub fn btn_generate_previews() -> String { crate::views::t("capture.btn_generate_previews") }
    pub fn btn_clear_discovered() -> String { crate::views::t("capture.btn_clear_discovered") }
    pub fn btn_clear_previews() -> String { crate::views::t("capture.btn_clear_previews") }
    pub fn btn_add_command() -> String { crate::views::t("capture.btn_add_command") }
    pub fn btn_add_default() -> String { crate::views::t("capture.btn_add_default") }
    pub fn btn_add_export_drive() -> String { crate::views::t("capture.btn_add_export_drive") }
    pub fn btn_load_project() -> String { crate::views::t("capture.btn_load_project") }
    pub fn btn_save() -> String { crate::views::t("capture.btn_save") }
    pub fn btn_save_as() -> String { crate::views::t("capture.btn_save_as") }
}
