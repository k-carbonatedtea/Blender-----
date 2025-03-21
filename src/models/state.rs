use super::operation::{FileOperation, ConversionStatus, ModInfo};
use eframe::epaint::Color32;
use std::path::PathBuf;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ModsTab {
    Mods,
    Package,
    Settings,
}

pub struct AppState {
    pub operations: Vec<FileOperation>,
    #[allow(dead_code)]
    pub current_operation: FileOperation,
    pub logs: Vec<String>,
    pub dark_mode: bool,
    pub show_settings: bool,
    pub show_about: bool,
    pub show_logs: bool,
    #[allow(dead_code)]
    pub file_dialog_open: bool,
    #[allow(dead_code)]
    pub processing_index: Option<usize>,
    #[allow(dead_code)]
    pub status_message: Option<String>,
    #[allow(dead_code)]
    pub status_color: Color32,
    pub auto_close: bool,
    pub auto_batch: bool,
    pub installed_mods: Vec<ModInfo>,
    pub show_mods: bool,
    pub show_mods_tab: ModsTab,
    pub main_mo_file: Option<PathBuf>,
    pub mods_directory: Option<PathBuf>,
    pub output_directory: Option<PathBuf>,
    pub cached_merged_po: Option<PathBuf>,
    pub needs_remerge: bool,
    pub ignore_main_mo_entries: bool,
    pub is_merging: bool,
    pub merge_progress: f32,
    pub merge_progress_anim: u32,
    pub show_help: bool,
    pub rename_mod_index: Option<usize>,
    pub rename_mod_name: String,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            operations: Vec::new(),
            current_operation: FileOperation::default(),
            logs: Vec::new(),
            dark_mode: true,
            show_settings: false,
            show_about: false,
            show_logs: true,
            file_dialog_open: false,
            processing_index: None,
            status_message: None,
            status_color: Color32::TRANSPARENT,
            auto_close: false,
            auto_batch: false,
            installed_mods: Vec::new(),
            show_mods: false,
            show_mods_tab: ModsTab::Mods,
            main_mo_file: None,
            mods_directory: None,
            output_directory: None,
            cached_merged_po: None,
            needs_remerge: false,
            ignore_main_mo_entries: false,
            is_merging: false,
            merge_progress: 0.0,
            merge_progress_anim: 0,
            show_help: false,
            rename_mod_index: None,
            rename_mod_name: String::new(),
        }
    }
}

impl AppState {
    pub fn add_log(&mut self, message: &str) {
        self.logs.push(message.to_string());
        
        // 限制日志数量，防止内存占用过大
        if self.logs.len() > 500 {
            self.logs.remove(0);
        }
    }
    
    /// 获取所有待处理任务的数量
    #[allow(dead_code)]
    pub fn pending_count(&self) -> usize {
        self.operations.iter()
            .filter(|op| op.status == ConversionStatus::Pending)
            .count()
    }
    
    /// 获取处理中任务的数量
    #[allow(dead_code)]
    pub fn processing_count(&self) -> usize {
        self.operations.iter()
            .filter(|op| op.status == ConversionStatus::Processing)
            .count()
    }
    
    /// 获取已完成任务的数量
    #[allow(dead_code)]
    pub fn completed_count(&self) -> usize {
        self.operations.iter()
            .filter(|op| op.status == ConversionStatus::Completed)
            .count()
    }
    
    /// 获取失败任务的数量
    #[allow(dead_code)]
    pub fn failed_count(&self) -> usize {
        self.operations.iter()
            .filter(|op| op.status == ConversionStatus::Failed)
            .count()
    }
} 