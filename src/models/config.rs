use std::path::PathBuf;
use std::fs;
use std::io;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;

/// 定义可选的主题
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum AppTheme {
    Light,        // 明亮主题
    Dark,         // 暗黑主题
    NightBlue,    // 夜间蓝
    Sepia,        // 护眼模式
    Forest,       // 森林绿
}

impl Default for AppTheme {
    fn default() -> Self {
        AppTheme::Dark
    }
}

/// 应用配置，用于存储和加载设置
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppConfig {
    // 主MO文件路径
    pub main_mo_file: Option<PathBuf>,
    // 语言包目录
    pub mods_directory: Option<PathBuf>,
    // 输出目录，用于存放合并后的MO文件
    pub output_directory: Option<PathBuf>,
    // 界面主题
    pub theme: AppTheme,
    // 为了向后兼容保留的深色模式标志
    pub dark_mode: bool,
    // 自动批处理
    pub auto_batch: bool,
    // 处理完成后自动关闭
    pub auto_close: bool,
    pub show_logs: bool,
    // 保存每个mod的启用状态 (文件名 -> 是否启用)
    pub saved_mods: HashMap<String, bool>,
    pub ignore_main_mo_entries: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            main_mo_file: None,
            mods_directory: None,
            output_directory: None,
            theme: AppTheme::default(),
            dark_mode: true,
            auto_batch: false,
            auto_close: false,
            show_logs: true,
            saved_mods: HashMap::new(),
            ignore_main_mo_entries: false,
        }
    }
}

impl AppConfig {
    /// 从本地文件加载配置
    pub fn load() -> Self {
        // 获取配置文件路径
        let config_path = get_config_path();
        
        // 尝试读取配置文件
        if let Ok(content) = fs::read_to_string(&config_path) {
            if let Ok(config) = serde_json::from_str::<AppConfig>(&content) {
                // 返回成功读取的配置
                return config;
            }
        }
        
        // 如果没有找到配置文件或者解析失败，返回默认配置
        let default_config = AppConfig::default();
        // 尝试保存默认配置
        let _ = default_config.save();
        
        default_config
    }
    
    /// 将配置保存到本地文件
    pub fn save(&self) -> io::Result<()> {
        // 获取配置文件路径
        let config_path = get_config_path();
        
        // 确保目录存在
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        // 将配置序列化为JSON
        let json = serde_json::to_string_pretty(self)?;
        
        // 写入文件
        fs::write(config_path, json)
    }
    
    /// 更新配置并保存
    #[allow(dead_code)]
    pub fn update_and_save(&mut self, new_config: AppConfig) -> io::Result<()> {
        *self = new_config;
        self.save()
    }
}

/// 获取配置文件路径
fn get_config_path() -> PathBuf {
    let cache_dir = if let Some(local_dir) = dirs::data_local_dir() {
        local_dir.join("BLMM")
    } else {
        // 如果无法获取系统本地数据目录，使用临时目录
        std::env::temp_dir().join("BLMM")
    };
    
    cache_dir.join("config.json")
}

/// 获取缓存目录路径
#[allow(dead_code)]
pub fn get_cache_dir() -> PathBuf {
    if let Some(local_dir) = dirs::data_local_dir() {
        local_dir.join("BLMM")
    } else {
        std::env::temp_dir().join("BLMM")
    }
}

/// 确保缓存目录存在
#[allow(dead_code)]
pub fn ensure_cache_dir() -> io::Result<PathBuf> {
    let cache_dir = get_cache_dir();
    fs::create_dir_all(&cache_dir)?;
    Ok(cache_dir)
} 