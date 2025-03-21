use std::path::PathBuf;
use std::time::{SystemTime, Instant, Duration};

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ConversionType {
    MoToPo,
    PoToMo,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ConversionStatus {
    Pending,
    Processing,
    Completed,
    Failed,
}

impl Default for ConversionStatus {
    fn default() -> Self {
        Self::Pending
    }
}

impl std::fmt::Display for ConversionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConversionStatus::Pending => write!(f, "等待处理"),
            ConversionStatus::Processing => write!(f, "处理中"),
            ConversionStatus::Completed => write!(f, "完成"),
            ConversionStatus::Failed => write!(f, "失败"),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ModStatus {
    Enabled,
    Disabled,
    NotInstalled,
}

impl Default for ModStatus {
    fn default() -> Self {
        Self::NotInstalled
    }
}

impl std::fmt::Display for ModStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModStatus::Enabled => write!(f, "已启用"),
            ModStatus::Disabled => write!(f, "已禁用"),
            ModStatus::NotInstalled => write!(f, "未安装"),
        }
    }
}

#[derive(Clone)]
pub struct FileOperation {
    pub input_file: Option<PathBuf>,
    pub input_path2: Option<PathBuf>,
    pub output_file: Option<PathBuf>,
    pub status: ConversionStatus,
    pub conversion_type: ConversionType,
    pub end_time: Option<chrono::DateTime<chrono::Local>>,
    pub start_time: Option<Instant>,
    pub duration: Option<f64>,
    pub elapsed_milliseconds: Option<u128>,
    pub error: Option<String>,
}

impl Default for FileOperation {
    fn default() -> Self {
        Self {
            input_file: None,
            input_path2: None,
            output_file: None,
            status: ConversionStatus::Pending,
            conversion_type: ConversionType::MoToPo,
            end_time: None,
            start_time: None,
            duration: None,
            elapsed_milliseconds: None,
            error: None,
        }
    }
}

#[derive(Clone)]
pub struct ModInfo {
    pub name: String,
    pub path: PathBuf,
    pub status: ModStatus,
    pub description: Option<String>,
    pub author: Option<String>,
    pub version: Option<String>,
    pub install_date: Option<chrono::DateTime<chrono::Local>>,
    pub last_updated: Option<chrono::DateTime<chrono::Local>>,
}

impl Default for ModInfo {
    fn default() -> Self {
        Self {
            name: String::new(),
            path: PathBuf::new(),
            status: ModStatus::default(),
            description: None,
            author: None,
            version: None,
            install_date: None,
            last_updated: None,
        }
    }
} 