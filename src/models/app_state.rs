use super::operation::FileOperation;

pub struct AppState {
    pub operations: Vec<FileOperation>,
    pub current_operation: FileOperation,
    pub logs: Vec<String>,
    pub dark_mode: bool,
    pub show_settings: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            operations: Vec::new(),
            current_operation: FileOperation::default(),
            logs: Vec::new(),
            dark_mode: true,
            show_settings: false,
        }
    }
}

impl AppState {
    pub fn add_log(&mut self, message: &str) {
        let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();
        self.logs.push(format!("[{}] {}", timestamp, message));
    }
} 