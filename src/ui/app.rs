use eframe::egui;
use egui::{Color32, RichText, Ui};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{SystemTime, UNIX_EPOCH, Duration, Instant};
use chrono::prelude::*;
use std::sync::Arc;
use rayon::ThreadPoolBuilder;
use walkdir;
use open;

use crate::models::{AppState, ConversionType, FileOperation, AppConfig, ConversionStatus, ModStatus, ModInfo, ModsTab};
use crate::converters::mo_converter::MoConverter;
use crate::converters::po_converter::PoConverter;
use crate::converters::po_merger;
use crate::converters::csv_converter::CsvConverter;

// 添加合并状态枚举
pub enum MergeStatus {
    Started,
    Progress(f32),
    Completed(PathBuf),
    Failed(String),
}

pub struct App {
    state: AppState,
    config: AppConfig,
    rx: Option<Receiver<(usize, Result<Duration, String>)>>,
    tx: Option<Sender<(usize, Result<Duration, String>)>>,
    merge_rx: Receiver<MergeStatus>,
    merge_tx: Sender<MergeStatus>,
    thread_pool: Arc<rayon::ThreadPool>,
    selected_category: String,
    search_text: String,
    show_install_dialog: bool,
    install_path: String,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        let (tx, rx) = channel();
        let (merge_tx, merge_rx) = channel();
        
        let thread_pool = ThreadPoolBuilder::new()
            .num_threads(num_cpus::get())
            .build()
            .unwrap();
        
        // 加载配置文件
        let mut config = AppConfig::load();
        
        // 创建应用状态并从配置中设置值
        let mut state = AppState::default();
        state.main_mo_file = config.main_mo_file.clone();
        
        // 设置固定的语言包目录
        let mods_dir = if let Some(local_dir) = dirs::data_local_dir() {
            local_dir.join("BLMM").join("mods")
        } else {
            std::env::temp_dir().join("BLMM").join("mods")
        };
        
        // 确保目录存在
        let _ = std::fs::create_dir_all(&mods_dir);
        
        // 设置目录
        state.mods_directory = Some(mods_dir.clone());
        config.mods_directory = Some(mods_dir);
        
        // 设置输出目录
        state.output_directory = config.output_directory.clone();
        
        // 为了向后兼容，根据主题设置dark_mode标志
        state.dark_mode = config.theme != crate::models::AppTheme::Light 
            && config.theme != crate::models::AppTheme::Sepia;
            
        state.auto_batch = config.auto_batch;
        state.auto_close = config.auto_close;
        state.show_logs = config.show_logs;
        state.ignore_main_mo_entries = config.ignore_main_mo_entries;
        
        // 默认显示语言包管理界面
        state.show_mods = true;
        state.show_mods_tab = ModsTab::Mods;
            
        let mut app = Self {
            state,
            config,
            rx: Some(rx),
            tx: Some(tx),
            merge_rx,
            merge_tx,
            thread_pool: Arc::new(thread_pool),
            selected_category: "Default".to_string(),
            search_text: String::new(),
            show_install_dialog: false,
            install_path: String::new(),
        };
        
        // 启动时自动扫描语言包目录
        app.scan_mods_directory();
        
        app
    }
    
    fn process_conversion_results(&mut self) {
        if let Some(rx) = &self.rx {
            if let Ok((index, result)) = rx.try_recv() {
                if index < self.state.operations.len() {
                    match result {
                        Ok(duration) => {
                            let now = Local::now();
                            self.state.operations[index].status = ConversionStatus::Completed;
                            self.state.operations[index].end_time = Some(now);
                            
                            // 计算耗时（毫秒和秒）
                            self.state.operations[index].duration = Some(duration.as_secs_f64());
                            self.state.operations[index].elapsed_milliseconds = Some(duration.as_millis());
                            
                            if let Some(output_file) = &self.state.operations[index].output_file {
                                self.state.add_log(&format!("转换成功: {}", output_file.display()));
                            }
                        }
                        Err(e) => {
                            self.state.operations[index].status = ConversionStatus::Failed;
                            self.state.operations[index].error = Some(e.clone());
                            self.state.add_log(&format!("转换失败: {}", e));
                        }
                    }
                    
                    // 检查是否有待处理的任务，如果有，则自动开始
                    if self.state.auto_batch {
                        let next_pending = self.state.operations.iter().enumerate()
                            .find(|(_, op)| op.status == ConversionStatus::Pending)
                            .map(|(i, _)| i);
                            
                        if let Some(next_index) = next_pending {
                            self.convert_file(next_index);
                        }
                    }
                } else {
                    self.state.add_log(&format!("错误: 收到无效的操作索引 {}", index));
                }
            }
        }
    }
    
    // 转换单个文件
    fn convert_file(&mut self, operation_index: usize) {
        if operation_index < self.state.operations.len() {
            // 添加调试日志
            self.state.add_log(&format!("开始转换任务 #{}", operation_index + 1));
            self.start_conversion(operation_index);
        }
    }
    
    fn start_conversion(&mut self, operation_index: usize) {
        if operation_index >= self.state.operations.len() {
            return;
        }
        
        let operation = self.state.operations[operation_index].clone();
        self.state.operations[operation_index].status = ConversionStatus::Processing;
        // 记录开始时间
        self.state.operations[operation_index].start_time = Some(Instant::now());
        
        if let Some(tx) = self.tx.clone() {
            let pool = self.thread_pool.clone();
            
            pool.spawn(move || {
                let start = Instant::now();
                
                let result = match operation.conversion_type {
                    ConversionType::MoToPo => {
                        if let (Some(input), Some(output)) = (&operation.input_file, &operation.output_file) {
                            MoConverter::convert_mo_to_po(input, output)
                                .map(|_| start.elapsed())
                        } else {
                            Err("输入或输出路径未设置".to_string())
                        }
                    }
                    ConversionType::PoToMo => {
                        if let (Some(input), Some(output)) = (&operation.input_file, &operation.output_file) {
                            PoConverter::convert_po_to_mo(input, output)
                                .map(|_| start.elapsed())
                        } else {
                            Err("输入或输出路径未设置".to_string())
                        }
                    }
                };
                
                let _ = tx.send((operation_index, result));
            });
        }
    }
    
    fn render_header(&mut self, ui: &mut Ui) {
        // 获取主题的强调色，用于标题
        let accent_color = crate::models::ThemeManager::get_accent_color(&self.config.theme);
        
        ui.heading(RichText::new("Blender字典合并管理器 By:凌川雪").color(accent_color));
        ui.label("快速将语言包PO文件转换并合并到MO文件中");
        
        ui.add_space(10.0);
    }
    
    fn render_operations(&mut self, ui: &mut Ui) {
        // 获取主题的强调色，用于标题
        let accent_color = crate::models::ThemeManager::get_accent_color(&self.config.theme);
        
        ui.heading(RichText::new("文件列表").color(accent_color));
        
        let mut to_delete = None;
        let mut start_conversion_index = None;
        let mut reset_completed_index = None;
        let mut retry_failed_index = None;
        let mut browse_input_index = None;
        let mut browse_output_index = None;
        
        for (i, operation) in self.state.operations.iter_mut().enumerate() {
        ui.horizontal(|ui| {
                ui.label(format!("{}. ", i + 1));
                
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label("类型: ");
                        ui.radio_value(&mut operation.conversion_type, ConversionType::MoToPo, "MO → PO");
                        ui.radio_value(&mut operation.conversion_type, ConversionType::PoToMo, "PO → MO");
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("输入: ");
                        if let Some(input) = &operation.input_file {
                            // 只显示文件名
                            let file_name = input.file_name()
                                .map_or_else(|| "[未知]".to_string(), 
                                          |name| name.to_string_lossy().to_string());
                            let response = ui.label(file_name);
                            
                            // 悬停时显示完整路径
                            let full_path = input.to_string_lossy().to_string();
                            response.on_hover_text(full_path);
                        } else {
                            ui.label("[未设置]");
                        }
                        
                        if ui.button("浏览").clicked() {
                            browse_input_index = Some(i);
                        }
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("输出: ");
                        if let Some(output) = &operation.output_file {
                            // 只显示文件名
                            let file_name = output.file_name()
                                .map_or_else(|| "[未知]".to_string(), 
                                          |name| name.to_string_lossy().to_string());
                            let response = ui.label(file_name);
                            
                            // 悬停时显示完整路径
                            let full_path = output.to_string_lossy().to_string();
                            response.on_hover_text(full_path);
                        } else {
                            ui.label("[未设置]");
                        }
                        
                        if ui.button("浏览").clicked() {
                            browse_output_index = Some(i);
                        }
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("状态: ");
                        match operation.status {
                            ConversionStatus::Pending => {
                                ui.label(operation.status.to_string());
                                if ui.button("开始").clicked() {
                                    start_conversion_index = Some(i);
                                }
                            },
                            ConversionStatus::Processing => {
                                if let Some(start) = operation.start_time {
                                    let elapsed = start.elapsed();
                                    ui.label(format!("处理中 ({:.2}秒)...", elapsed.as_secs_f64()));
                                } else {
                                    ui.label("处理中...");
                                }
                            },
                            ConversionStatus::Completed => {
                                // 获取成功状态颜色
                                let (_success_color, _warning_color, _error_color, _info_color) =
                                    crate::models::ThemeManager::get_status_colors();
                                ui.label(RichText::new("完成").color(Color32::LIGHT_BLUE));
                                if let Some(duration) = operation.duration {
                                    ui.label(format!("({:.3}秒)", duration));
                                }
                                if let Some(elapsed_ms) = operation.elapsed_milliseconds {
                                    ui.label(format!("[{}毫秒]", elapsed_ms));
                                }
                                
                                if ui.button("再次转换").clicked() {
                                    reset_completed_index = Some(i);
                                }
                            },
                            ConversionStatus::Failed => {
                                // 获取错误状态颜色
                                let (_success_color, _warning_color, _error_color, _info_color) =
                                    crate::models::ThemeManager::get_status_colors();
                                ui.label(RichText::new("失败").color(Color32::RED));
                                if let Some(error) = &operation.error {
                                    ui.label(RichText::new(error).color(Color32::RED));
                                }
                                
                                if ui.button("重试").clicked() {
                                    retry_failed_index = Some(i);
                                }
                            },
                        }
                        
                        if ui.button("删除").clicked() {
                            to_delete = Some(i);
                        }
                    });
                });
            });
            
        ui.separator();
    }
    
        // 处理浏览输入文件
        if let Some(i) = browse_input_index {
            if i < self.state.operations.len() {
                let operation = &mut self.state.operations[i];
                let ext = match operation.conversion_type {
                    ConversionType::MoToPo => "mo",
                    _ => "po"
                };
                
                if let Some(file) = rfd::FileDialog::new()
                    .add_filter("文件", &[ext])
                    .pick_file() {
                        operation.input_file = Some(file.clone());
                        
                        // 自动设置输出文件名
                        let mut output_file = file.clone();
                        let new_ext = if ext == "mo" { "po" } else { "mo" };
                        output_file.set_extension(new_ext);
                        operation.output_file = Some(output_file);
                    }
            }
        }
        
        // 处理浏览输出文件
        if let Some(i) = browse_output_index {
            if i < self.state.operations.len() {
                let operation = &mut self.state.operations[i];
                let ext = match operation.conversion_type {
                    ConversionType::MoToPo => "po",
                    ConversionType::PoToMo => "mo",
                };
                
                if let Some(file) = rfd::FileDialog::new()
                    .add_filter("文件", &[ext])
                    .save_file() {
                        operation.output_file = Some(file);
                    }
            }
        }
        
        // 处理重置操作
        if let Some(i) = reset_completed_index {
            if i < self.state.operations.len() {
                let operation = &mut self.state.operations[i];
                operation.status = ConversionStatus::Pending;
                operation.start_time = None;
                operation.end_time = None;
                operation.duration = None;
                operation.elapsed_milliseconds = None;
                operation.error = None;
                self.convert_file(i);
            }
        }
        
        // 处理重试操作
        if let Some(i) = retry_failed_index {
            if i < self.state.operations.len() {
                let operation = &mut self.state.operations[i];
                operation.status = ConversionStatus::Pending;
                operation.error = None;
                self.convert_file(i);
            }
        }
        
        // 处理开始转换操作
        if let Some(i) = start_conversion_index {
            self.convert_file(i);
        }
        
        // 处理删除操作
        if let Some(i) = to_delete {
            self.state.operations.remove(i);
        }
        
        ui.horizontal(|ui| {
            let accent_color = crate::models::ThemeManager::get_accent_color(&self.config.theme);
            
            // 样式化的添加按钮
            if ui.add(egui::Button::new(RichText::new("添加MO→PO任务").color(accent_color))
                     .min_size(egui::vec2(140.0, 24.0)))
                     .clicked() {
                self.open_specific_file_dialog(ConversionType::MoToPo);
            }
            
            if ui.add(egui::Button::new(RichText::new("添加PO→MO任务").color(accent_color))
                     .min_size(egui::vec2(140.0, 24.0)))
                     .clicked() {
                self.open_specific_file_dialog(ConversionType::PoToMo);
            }
            
            let (_success_color, _warning_color, _error_color, _info_color) =
                crate::models::ThemeManager::get_status_colors();
            
            if ui.add(egui::Button::new(RichText::new("批量处理").color(Color32::LIGHT_BLUE))
                     .min_size(egui::vec2(100.0, 24.0)))
                     .clicked() {
                self.batch_process();
            }
            
            ui.separator();
            
            ui.checkbox(&mut self.state.auto_close, "处理完成后自动关闭");
            ui.checkbox(&mut self.state.auto_batch, "自动批处理");
            
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(format!("线程池: {} 线程", num_cpus::get()));
            });
        });
    }
    
    fn render_logs(&mut self, ui: &mut Ui) {
        ui.collapsing("日志", |ui| {
            egui::ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                for log in &self.state.logs {
                    ui.label(log);
                }
                
                // 自动滚动到底部
                if !self.state.logs.is_empty() {
                    ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                }
            });
        });
    }
    
    fn render_settings(&mut self, _ctx: &egui::Context) {
        if self.state.show_settings {
            // 当通过菜单打开设置窗口时，自动切换到语言包管理器的设置标签页
            self.state.show_mods = true;
            self.state.show_mods_tab = ModsTab::Settings;
            self.state.show_settings = false;
        }
    }
    
    // 打开文件选择对话框
    #[allow(dead_code)]
    fn open_file_dialog(&mut self) {
        // 创建一个新的操作，使用当前应用状态的转换类型
        let mut new_operation = FileOperation::default();
        
        // 根据当前转换类型设置文件过滤器
        let ext = match new_operation.conversion_type {
            ConversionType::MoToPo => "mo",
            _ => "po"
        };
        
        // 添加调试日志
        self.state.add_log(&format!("正在选择{}文件...", ext));
        
        // 打开文件选择对话框
        if let Some(file) = rfd::FileDialog::new()
            .add_filter("文件", &[ext])
            .set_title(&format!("选择{}文件", ext))
                                .pick_file() {
                new_operation.input_file = Some(file.clone());
                                    
                                    // 自动设置输出文件名
                let mut output_file = file.clone();
                let new_ext = if ext == "mo" { "po" } else { "mo" };
                output_file.set_extension(new_ext);
                new_operation.output_file = Some(output_file.clone());
                
                // 添加到操作列表
                self.state.operations.push(new_operation);
                self.state.add_log(&format!("已添加新任务: {} → {}", 
                    file.display(), 
                    output_file.display()));
            } else {
                self.state.add_log("文件选择已取消");
            }
    }
    
    // 批量处理所有待处理的文件
    fn batch_process(&mut self) {
        // 获取所有待处理的文件索引
        let pending_indices: Vec<usize> = self.state.operations.iter().enumerate()
            .filter(|(_, op)| op.status == ConversionStatus::Pending)
            .map(|(i, _)| i)
            .collect();
            
        // 开始处理第一个文件
        if let Some(&index) = pending_indices.first() {
            self.convert_file(index);
        }
    }

    #[allow(dead_code)]
    fn format_time(dt: &DateTime<Local>) -> String {
        dt.format("%Y-%m-%d %H:%M:%S").to_string()
    }

    // 添加新的文件选择对话框函数，可以指定转换类型
    fn open_specific_file_dialog(&mut self, conversion_type: ConversionType) {
        // 创建一个新的操作，设置指定的转换类型
        let mut new_operation = FileOperation::default();
        new_operation.conversion_type = conversion_type;
        
        // 根据转换类型设置文件过滤器
        let ext = match conversion_type {
            ConversionType::MoToPo => "mo",
            ConversionType::PoToMo => "po",
        };
        
        // 添加调试日志
        self.state.add_log(&format!("正在选择{}文件...", ext));
        
        // 打开文件选择对话框
        if let Some(file) = rfd::FileDialog::new()
            .add_filter("文件", &[ext])
            .set_title(&format!("选择{}文件", ext))
            .pick_file() 
        {
            new_operation.input_file = Some(file.clone());
            
            // 自动设置输出文件名
            let mut output_file = file.clone();
            let new_ext = if ext == "mo" { "po" } else { "mo" };
            output_file.set_extension(new_ext);
            new_operation.output_file = Some(output_file.clone());
            
            // 添加到操作列表
            self.state.operations.push(new_operation);
            self.state.add_log(&format!("已添加新任务: {} → {}", 
                file.display(), 
                output_file.display()));
        } else {
            self.state.add_log("文件选择已取消");
        }
    }

    fn render_mods(&mut self, ui: &mut Ui) {
        // 获取主题强调色
        let accent_color = crate::models::ThemeManager::get_accent_color(&self.config.theme);
        
        // Top menu bar
        ui.horizontal(|ui| {
            if ui.selectable_label(self.state.show_mods_tab == ModsTab::Mods, 
                           RichText::new("语言包").color(
                               if self.state.show_mods_tab == ModsTab::Mods { accent_color } 
                               else { ui.style().visuals.text_color() }
                           )).clicked() {
                self.state.show_mods_tab = ModsTab::Mods;
            }
            if ui.selectable_label(self.state.show_mods_tab == ModsTab::Package, 
                           RichText::new("仓库").color(
                               if self.state.show_mods_tab == ModsTab::Package { accent_color } 
                               else { ui.style().visuals.text_color() }
                           )).clicked() {
                self.state.show_mods_tab = ModsTab::Package;
            }
            if ui.selectable_label(self.state.show_mods_tab == ModsTab::Settings, 
                           RichText::new("设置").color(
                               if self.state.show_mods_tab == ModsTab::Settings { accent_color } 
                               else { ui.style().visuals.text_color() }
                           )).clicked() {
                self.state.show_mods_tab = ModsTab::Settings;
            }
            if ui.selectable_label(self.state.show_mods_tab == ModsTab::OpenAI, 
                           RichText::new("AI 翻译").color(
                               if self.state.show_mods_tab == ModsTab::OpenAI { accent_color } 
                               else { ui.style().visuals.text_color() }
                           )).clicked() {
                self.state.show_mods_tab = ModsTab::OpenAI;
            }
        });

        ui.separator();

        match self.state.show_mods_tab {
            ModsTab::Mods => self.render_mods_list(ui),
            ModsTab::Package => self.render_package_tab(ui),
            ModsTab::Settings => self.render_mod_settings(ui),
            ModsTab::OpenAI => self.render_openai_tab(ui),
        }
    }

    fn render_mods_list(&mut self, ui: &mut Ui) {
        // 获取主题颜色
        let accent_color = crate::models::ThemeManager::get_accent_color(&self.config.theme);
        let (_success_color, _warning_color, _error_color, _info_color) =
            crate::models::ThemeManager::get_status_colors();
        
        // Top controls
        ui.horizontal(|ui| {
            ui.push_id("mods_combobox", |ui| {
                egui::ComboBox::from_id_source("profile_selector")
                    .selected_text(&self.selected_category)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.selected_category, "Default".to_string(), "默认");
                        // Could add other categories here
                    });
            });

            if ui.button("+").clicked() {
                // Add new profile
            }
            if ui.button("≡").clicked() {
                // Show profile options
            }

            // 添加"安装语言包"按钮，使用强调色
            if ui.add(egui::Button::new(RichText::new("安装模组包(可多选)").color(accent_color))
                .min_size(egui::vec2(150.0, 24.0)))
                .clicked() {
                self.install_new_mod();
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let enabled_count = self.state.installed_mods.iter().filter(|m| m.status == ModStatus::Enabled).count();
                ui.label(format!("{} 语言包 / {} 已启用", self.state.installed_mods.len(), enabled_count));
                
                // 添加"导出基础文件"按钮 - 放在合并按钮旁边
                ui.add_space(5.0); // 增加一点间距
                if ui.add(egui::Button::new(RichText::new("导出基础文件").color(Color32::LIGHT_GREEN))
                    .min_size(egui::vec2(110.0, 28.0)))
                    .on_hover_text("直接导出基础MO文件（不合并），从名称中移除base")
                    .clicked() {
                    self.export_base_mo_file();
                }
                
                // 当有启用的语言包时或需要重新合并时显示合并按钮
                if enabled_count > 0 || self.state.needs_remerge {
                    // 如果需要重新合并，显示"重新合并"按钮并使用不同颜色
                    ui.push_id("remerge_button", |ui| {
                        // 获取状态颜色
                        let (_success_color, _warning_color, _error_color, _info_color) =
                            crate::models::ThemeManager::get_status_colors();
                        
                        // 如果正在合并中，显示进度动画
                        if self.state.is_merging {
                            let progress_text = if self.state.merge_progress >= 0.99 {
                                "合并完成".to_string()
                            } else {
                                // 显示百分比进度
                                let percent = (self.state.merge_progress * 100.0) as i32;
                                format!("合并中 {}%", percent)
                            };
                            
                            ui.add(egui::ProgressBar::new(self.state.merge_progress)
                                .text(RichText::new(progress_text).color(Color32::LIGHT_BLUE))
                                .fill(Color32::LIGHT_BLUE)
                                .animate(true));
                        } else {
                            let button_text = if self.state.needs_remerge {
                                RichText::new("重新合并").color(Color32::LIGHT_BLUE)
                            } else {
                                RichText::new("合并模组").color(Color32::LIGHT_BLUE)
                            };
                            
                            let button = egui::Button::new(button_text)
                                .min_size(egui::vec2(130.0, 28.0));
                                
                            if ui.add(button).clicked() {
                                // 设置合并状态并启动线程
                                self.state.is_merging = true;
                                self.state.merge_progress = 0.0;
                                self.state.merge_progress_anim = 0;
                                
                                // 在线程中执行合并，以避免UI冻结
                                let tx = self.merge_tx.clone();
                                let po_files: Vec<PathBuf> = self.state.installed_mods.iter()
                                    .filter(|m| m.status == ModStatus::Enabled)
                                    .map(|m| m.path.clone())
                                    .collect();
                                let ignore_main = self.state.ignore_main_mo_entries;
                                
                                self.thread_pool.spawn(move || {
                                    // 通知开始
                                    let _ = tx.send(MergeStatus::Started);
                                    
                                    // 创建缓存目录
                                    let cache_dir = if let Some(local_dir) = dirs::data_local_dir() {
                                        local_dir.join("BLMM").join("cache")
                                    } else {
                                        std::env::temp_dir().join("BLMM").join("cache")
                                    };
                                    
                                    if let Err(e) = std::fs::create_dir_all(&cache_dir) {
                                        let _ = tx.send(MergeStatus::Failed(format!("创建缓存目录失败: {}", e)));
                                        return;
                                    }
                                    
                                    // 缓存合并PO的路径
                                    let cached_po_path = cache_dir.join("cached_merged.po");
                                    
                                    // 更新进度 - 添加更多的进度点
                                    let _ = tx.send(MergeStatus::Progress(0.1)); // 10%
                                    std::thread::sleep(std::time::Duration::from_millis(100));
                                    
                                    let _ = tx.send(MergeStatus::Progress(0.2)); // 20%
                                    std::thread::sleep(std::time::Duration::from_millis(100));
                                    
                                    // 合并PO文件
                                    match po_merger::merge_po_files(&po_files, &cached_po_path, ignore_main) {
                                        Ok(_) => {
                                            // 更新进度 - 添加更多的进度点
                                            let _ = tx.send(MergeStatus::Progress(0.3)); // 30%
                                            std::thread::sleep(std::time::Duration::from_millis(100));
                                            
                                            let _ = tx.send(MergeStatus::Progress(0.4)); // 40%
                                            std::thread::sleep(std::time::Duration::from_millis(100));
                                            
                                            let _ = tx.send(MergeStatus::Progress(0.5)); // 50%
                                            std::thread::sleep(std::time::Duration::from_millis(100));

                                            let _ = tx.send(MergeStatus::Progress(0.6)); // 60%
                                            std::thread::sleep(std::time::Duration::from_millis(100));

                                            let _ = tx.send(MergeStatus::Progress(0.7)); // 70%
                                            std::thread::sleep(std::time::Duration::from_millis(100));
                                            
                                            let _ = tx.send(MergeStatus::Progress(0.8)); // 80%
                                            std::thread::sleep(std::time::Duration::from_millis(100));

                                            let _ = tx.send(MergeStatus::Progress(0.9)); // 90%
                                            
                                            
                                            // 完成
                                            let _ = tx.send(MergeStatus::Completed(cached_po_path));
                                        },
                                        Err(e) => {
                                            let _ = tx.send(MergeStatus::Failed(format!("合并PO文件失败: {}", e)));
                                        }
                                    }
                                });
                            }
                        }
                    });
                }
            });
        });

        // Table header
        ui.horizontal(|ui| {
            ui.add_space(30.0); // Checkbox column
            ui.label("语言包名称").on_hover_text("按名称排序");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label("优先级 ▼").on_hover_text("数字越小优先级越高，优先级高的翻译将覆盖优先级低的翻译");
                ui.label("版本");
                ui.label("类别");
            });
        });

        ui.separator();

        // 计算合适的高度，保留足够空间给日志区域
        let available_height = ui.available_height();
        // 留出日志区域高度（如果日志可见）
        let log_area_height = if self.state.show_logs { 220.0 } else { 0.0 };
        let mods_list_height = available_height - log_area_height - 40.0; // 额外留出一些空间给UI元素

        // Mods list
        let mut to_enable = None;
        let mut to_disable = None;
        let mut to_uninstall = None;

        // 拖放功能已移除
        // 根据用户要求，已删除拖拽安装PO文件的功能

        // 如果没有mods，显示一个提示区域
        if self.state.installed_mods.is_empty() {
            let available_rect = ui.available_rect_before_wrap();
            let empty_area_rect = egui::Rect::from_min_size(
                available_rect.min,
                egui::Vec2::new(available_rect.width(), mods_list_height)
            );
            
            let empty_area_response = ui.allocate_rect(
                empty_area_rect,
                egui::Sense::hover()
            );
            
            let painter = ui.painter();
            let rect = empty_area_response.rect;
            
            painter.rect_stroke(
                rect,
                5.0,
                egui::Stroke::new(1.0, Color32::from_rgb(100, 100, 100))
            );
            
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "点击上方\"安装语言包\"按钮添加语言包",
                egui::TextStyle::Body.resolve(ui.style()),
                Color32::from_rgb(180, 180, 180)
            );
        } else {
            // 如果有mods，显示一个可滚动列表
            ui.push_id("mods_list_scroll", |ui| {
                egui::ScrollArea::vertical().max_height(mods_list_height).show(ui, |ui| {
                    let mut move_up_index = None;
                    let mut move_down_index = None;
                    
                    for (index, mod_info) in self.state.installed_mods.iter().enumerate() {
                        ui.push_id(index, |ui| {
                            let row_response = ui.horizontal(|ui| {
                                // Checkbox for enabled/disabled
                                let mut is_enabled = mod_info.status == ModStatus::Enabled;
                                
                                // 使用on_change来检测复选框状态变化
                                if ui.checkbox(&mut is_enabled, "").changed() {
                                    // 只有当状态确实发生变化时才添加到待处理队列
                                    if is_enabled {
                                        to_enable = Some(index);
                                    } else {
                                        to_disable = Some(index);
                                    }
                                    
                                    // 直接在此处设置needs_remerge标志
                                    self.state.needs_remerge = true;
                                }

                                // Color the selected row
                                let text_color = if is_enabled { Color32::LIGHT_BLUE } else { ui.style().visuals.text_color() };
                                
                                // Display the name without file extension
                                let display_name = if mod_info.name.to_lowercase().ends_with(".po") {
                                    if let Some(pos) = mod_info.name.rfind('.') {
                                        &mod_info.name[0..pos]
                                    } else {
                                        &mod_info.name
                                    }
                                } else {
                                    &mod_info.name
                                };
                                
                                ui.colored_label(text_color, display_name);

                                // Right side of the row
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    // 添加上下移动按钮
                                    let can_move_down = index < self.state.installed_mods.len() - 1;
                                    let can_move_up = index > 0;
                                    
                                    if ui.add_enabled(can_move_down, egui::Button::new("▼")).clicked() {
                                        // 下移
                                        move_down_index = Some(index);
                                    }
                                    
                                    if ui.add_enabled(can_move_up, egui::Button::new("▲")).clicked() {
                                        // 上移
                                        move_up_index = Some(index);
                                    }
                                    
                                    ui.label(format!("{}", index)); // Priority
                                    ui.label(mod_info.version.as_deref().unwrap_or("1.0.0")); // Version
                                    
                                    // Display original file type if available, otherwise use description or default
                                    if let Some(orig_type) = &mod_info.original_type {
                                        ui.label(format!("从{}转换的PO文件", orig_type));
                                    } else {
                                        ui.label(mod_info.description.as_deref().unwrap_or("语言包")); // Category
                                    }
                                });
                            });

                            // Highlight when hovered
                            if row_response.response.hovered() {
                                row_response.response.clone().highlight();
                            }

                            // Context menu
                            row_response.response.context_menu(|ui| {
                                if ui.button("重命名").clicked() {
                                    // 打开重命名对话框
                                    let mod_name = &self.state.installed_mods[index].name;
                                    self.state.rename_mod_index = Some(index);
                                    
                                    // Strip the .po extension for display
                                    let display_name = if mod_name.to_lowercase().ends_with(".po") {
                                        if let Some(pos) = mod_name.rfind('.') {
                                            mod_name[0..pos].to_string()
                                        } else {
                                            mod_name.clone()
                                        }
                                    } else {
                                        mod_name.clone()
                                    };
                                    
                                    self.state.rename_mod_name = display_name;
                                    ui.close_menu();
                                }
                                
                                if ui.button("卸载").clicked() {
                                    to_uninstall = Some(index);
                                    ui.close_menu();
                                }
                            });
                        });

                        ui.separator();
                    }
                    
                    // 处理优先级移动操作
                    if let Some(index) = move_up_index {
                        if index > 0 {
                            self.state.installed_mods.swap(index, index - 1);
                            self.state.needs_remerge = true;
                        }
                    }
                    
                    if let Some(index) = move_down_index {
                        if index < self.state.installed_mods.len() - 1 {
                            self.state.installed_mods.swap(index, index + 1);
                            self.state.needs_remerge = true;
                        }
                    }
                });
            });
        }

        // Handle mod state changes
        if let Some(index) = to_enable {
            self.enable_mod(index);
        }
        
        if let Some(index) = to_disable {
            self.disable_mod(index);
        }
        
        if let Some(index) = to_uninstall {
            self.uninstall_mod(index);
        }

        // 显示日志区域（如果启用）
        if self.state.show_logs {
            ui.separator();
            ui.horizontal(|ui| {
                ui.heading("日志");
                if ui.button("清空").clicked() {
                    self.state.logs.clear();
                }
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.text_edit_singleline(&mut self.search_text).on_hover_text("搜索日志");
                    ui.label("搜索:");
                    
                    if ui.button(if self.state.show_logs { "隐藏日志" } else { "显示日志" }).clicked() {
                        self.state.show_logs = !self.state.show_logs;
                        // 保存配置
                        self.config.show_logs = self.state.show_logs;
                        self.config.save().ok();
                    }
                });
            });

            ui.push_id("logs_scroll", |ui| {
                egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                    for log in &self.state.logs {
                        if self.search_text.is_empty() || log.to_lowercase().contains(&self.search_text.to_lowercase()) {
                            ui.label(log);
                        }
                    }
                
                    // Auto-scroll to the latest log
                    if !self.state.logs.is_empty() {
                        ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                    }
                });
            });
        }
        
        // Install dialog
        self.render_install_dialog(ui.ctx());
    }

    // Get or create mods cache directory
    fn get_or_create_mods_cache_dir(&self) -> Option<PathBuf> {
        // 使用 AppData\Local\BLMM\mods 目录作为默认目录
        let cache_dir = if let Some(local_dir) = dirs::data_local_dir() {
            local_dir.join("BLMM").join("mods")
        } else {
            // 如果无法获取系统本地数据目录，使用临时目录
            std::env::temp_dir().join("BLMM").join("mods")
        };
        
        // 确保目录存在
        if let Err(e) = std::fs::create_dir_all(&cache_dir) {
            eprintln!("创建语言包缓存目录失败: {}", e);
            return None;
        }
        
        Some(cache_dir)
    }

    // Generate the cached merged PO file from selected mods
    #[allow(dead_code)]
    fn generate_cached_merged_po(&mut self) {
        // 该方法现在被线程化处理，这里不需要任何操作
        // 所有逻辑都移到了点击事件和process_merge_status方法中
    }

    // Apply the cached merged PO file to the main MO file
    fn apply_merged_po_to_mo(&mut self) {
        // Check if we have main MO file and cached merged PO
        if self.state.main_mo_file.is_none() {
            self.state.add_log("错误: 请先在设置中设置主MO文件");
            return;
        }
        
        if self.state.cached_merged_po.is_none() {
            self.state.add_log("错误: 没有可用的合并PO缓存，请先点击'合并选中PO'");
            return;
        }
        
        let main_mo_file = self.state.main_mo_file.clone().unwrap();
        let cached_po_file = self.state.cached_merged_po.clone().unwrap();
        
        // 使用 AppData\Local\BLMM\cache 目录
        let cache_dir = if let Some(local_dir) = dirs::data_local_dir() {
            local_dir.join("BLMM").join("cache")
        } else {
            // 如果无法获取系统本地数据目录，使用临时目录
            std::env::temp_dir().join("BLMM").join("cache")
        };
        
        // 确保缓存目录存在
        if let Err(e) = std::fs::create_dir_all(&cache_dir) {
            self.state.add_log(&format!("创建缓存目录失败: {}", e));
            return;
        }
        
        // Create output MO file path - 使用用户设置的输出目录或桌面上的"BLMM导出"文件夹
        let output_mo_path = if let Some(output_dir) = &self.state.output_directory {
            // 使用用户设置的输出目录
            if let Err(e) = std::fs::create_dir_all(output_dir) {
                self.state.add_log(&format!("创建输出目录失败: {}", e));
                // 如果创建目录失败，回退到桌面上的"BLMM导出"文件夹
                self.create_default_output_directory()
                    .map(|dir| dir.join("blender.mo"))
                    .unwrap_or_else(|| cache_dir.join("blender.mo"))
            } else {
                // 使用设置的输出目录
                output_dir.join("blender.mo")
            }
        } else {
            // 未设置输出目录，使用桌面上的"BLMM导出"文件夹
            self.create_default_output_directory()
                .map(|dir| dir.join("blender.mo"))
                .unwrap_or_else(|| {
                    // 如果创建桌面文件夹失败，回退到主MO文件所在目录
                    if let Some(parent) = main_mo_file.parent() {
                        parent.join("blender.mo")
                    } else {
                        cache_dir.join("blender.mo")
                    }
                })
        };
        
        // Convert the main MO file to PO first
        let main_po_path = cache_dir.join("main.po");
        self.state.add_log("正在将主MO文件转换为PO格式...");
        
        match MoConverter::convert_mo_to_po(&main_mo_file, &main_po_path) {
            Ok(_) => {
                self.state.add_log("主MO文件转换成功，准备与缓存PO合并...");
                
                // Merge main PO with cached PO
                let all_po_files = vec![main_po_path.clone(), cached_po_file];
                let final_merged_po = cache_dir.join("final_merged.po");
                
                // 记录是否使用了忽略主mo条目的选项
                let ignore_msg = if self.state.ignore_main_mo_entries {
                    "（已启用忽略主MO条目模式）"
                } else {
                    ""
                };
                
                match po_merger::merge_po_files(&all_po_files, &final_merged_po, self.state.ignore_main_mo_entries) {
                    Ok(_) => {
                        self.state.add_log(&format!("最终PO文件合并成功{}，正在转换为MO格式...", ignore_msg));
                        
                        // Convert the final merged PO to MO
                        match PoConverter::convert_po_to_mo(&final_merged_po, &output_mo_path) {
                            Ok(_) => {
                                // 获取输出目录用于日志显示
                                let output_dir = output_mo_path.parent()
                                    .map_or_else(|| "[未知目录]".to_string(), 
                                               |dir| dir.to_string_lossy().to_string());
                                let file_name = output_mo_path.file_name()
                                    .map_or_else(|| "[未知文件]".to_string(), 
                                               |name| name.to_string_lossy().to_string());
                                self.state.add_log(&format!("合并完成! 新MO文件已保存到: {}/{}", output_dir, file_name));
                            },
                            Err(e) => {
                                self.state.add_log(&format!("将合并后的PO转换为MO失败: {}", e));
                            }
                        }
                    },
                    Err(e) => {
                        self.state.add_log(&format!("最终PO文件合并失败: {}", e));
                    }
                }
            },
            Err(e) => {
                self.state.add_log(&format!("将主MO文件转换为PO失败: {}", e));
            }
        }
    }

    // 创建默认的输出目录（桌面上的"BLMM导出"文件夹）
    fn create_default_output_directory(&mut self) -> Option<PathBuf> {
        // 获取桌面路径
        let desktop_dir = dirs::desktop_dir()?;
        let default_output_dir = desktop_dir.join("BLMM导出");
        
        // 尝试创建目录
        match std::fs::create_dir_all(&default_output_dir) {
            Ok(_) => {
                self.state.add_log(&format!("已创建默认输出目录: {}", default_output_dir.to_string_lossy()));
                Some(default_output_dir)
            },
            Err(e) => {
                self.state.add_log(&format!("创建默认输出目录失败: {}", e));
                None
            }
        }
    }

    // Restore the refresh_mods_list function
    #[allow(dead_code)]
    fn refresh_mods_list(&mut self) {
        self.scan_mods_directory();
    }

    fn render_install_dialog(&mut self, ctx: &egui::Context) {
        if self.show_install_dialog {
            egui::Window::new("安装")
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("📁").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .set_title("Choose Download Directory")
                                .pick_folder() {
                                self.install_path = path.display().to_string();
                            }
                        }
                        ui.text_edit_singleline(&mut self.install_path);
                    });

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let file_types = ["girly_animation_pack_v107_switch.bnp", 
                                         "grav boosters-6816-2-0-1-1702399400.zip",
                                         "hyliapack.bnp", 
                                         "Legendary Modification-1379-1-0-2-1697809243.7z"];
                                         
                        for file in file_types {
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut false, "");
                                ui.label(file);
                            });
                        }
                    });

                    if ui.button("关闭").clicked() {
                        self.show_install_dialog = false;
                    }
                });
        }
    }

    fn render_package_tab(&mut self, ui: &mut Ui) {
        ui.heading("语言包管理");
        
        ui.horizontal(|ui| {
            if ui.button("浏览可用语言包").clicked() {
                // This would connect to a repository or show local packages
            }
            
            if ui.button("更新语言包列表").clicked() {
                // This would refresh available packages
            }
        });
        
        ui.separator();
        
        ui.label("没有可用的语言包。请更新语言包列表或检查网络连接。");
    }

    fn render_mod_settings(&mut self, ui: &mut Ui) {
        // 获取主题的强调色
        let accent_color = crate::models::ThemeManager::get_accent_color(&self.config.theme);
        
        ui.heading(RichText::new("设置").color(accent_color));
        
        // 保存原始配置值，以检测更改
        let orig_main_mo_file = self.state.main_mo_file.clone();
        let orig_dark_mode = self.state.dark_mode;
        let orig_auto_batch = self.state.auto_batch;
        let orig_auto_close = self.state.auto_close;
        let orig_show_logs = self.state.show_logs;
        let orig_ignore_main_mo_entries = self.state.ignore_main_mo_entries;
        let orig_theme = self.config.theme.clone();
        
        // 添加主题设置部分
        ui.collapsing("界面主题", |ui| {
            let theme_names = crate::models::ThemeManager::get_theme_names();
            
            // 绘制主题选择按钮
            ui.horizontal_wrapped(|ui| {
                for (name, theme) in theme_names {
                    // 设置按钮样式
                    let mut button = egui::Button::new(name);
                    
                    // 根据主题类型设置不同的按钮颜色
                    if self.config.theme == theme {
                        button = button.fill(crate::models::ThemeManager::get_accent_color(&theme));
                    }
                    
                    // 添加按钮并处理点击事件
                    if ui.add(button).clicked() {
                        self.config.theme = theme.clone();
                        self.state.dark_mode = theme != crate::models::AppTheme::Light 
                            && theme != crate::models::AppTheme::Sepia;
                    }
                }
            });
        });
        
        ui.separator();
        
        // 主MO文件设置部分
        ui.heading("基础MO文件");
        
        ui.horizontal(|ui| {
            ui.label("主MO文件:");
            
            if let Some(mo_file) = &self.state.main_mo_file {
                // 显示文件名
                let file_name = mo_file.file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_else(|| mo_file.display().to_string());
                let response = ui.label(file_name);
                
                // 悬停时显示完整路径
                let full_path = mo_file.to_string_lossy().to_string();
                response.on_hover_text(full_path);
            } else {
                ui.label("[未设置]");
            }
            
            if ui.button("选择MO文件").clicked() {
                if let Some(mo_path) = rfd::FileDialog::new()
                    .add_filter("MO文件", &["mo"])
                    .set_title("选择Blender的mo文件")
                    .pick_file() {
                    
                    // 保存主MO文件路径
                    self.state.main_mo_file = Some(mo_path.clone());
                    self.config.main_mo_file = Some(mo_path.clone());
                    
                    // 清除合并缓存，因为主MO文件已更改
                    self.state.cached_merged_po = None;
                    self.state.needs_remerge = true;
                    
                    // 添加日志
                    self.state.add_log(&format!("已设置主MO文件: {}", mo_path.display()));
                    
                    // 将文件克隆到BLMM文件夹
                    self.clone_main_mo_to_blmm(&mo_path);
                }
            }
            
            if ui.button("自动查找").clicked() {
                self.auto_locate_blender_mo_file();
            }
            
            if ui.button("清除").clicked() {
                self.state.main_mo_file = None;
                self.config.main_mo_file = None;
                
                // 清除合并缓存，因为主MO文件已更改
                self.state.cached_merged_po = None;
                self.state.needs_remerge = true;
                
                self.state.add_log("已清除主MO文件设置");
            }
        });
        
        // 添加导出基础MO文件的按钮和说明
        ui.horizontal(|ui| {
            if ui.button("导出基础文件").clicked() {
                self.export_base_mo_file();
            }
            ui.label("(将当前的基础MO文件导出为独立文件，不做任何合并)");
        });
        
        ui.add_space(4.0);
        
        ui.horizontal(|ui| {
            ui.label("主MO文件描述:").on_hover_text("主MO文件是Blender的原始翻译文件，通常位于Blender安装目录的'datafiles/locale/zh_CN/LC_MESSAGES/'下，名为'blender.mo'");
        });
        
        ui.separator();
        
        // 输出目录设置部分
        ui.heading("输出设置");
        
        ui.horizontal(|ui| {
            ui.label("输出目录:");
            
            if let Some(output_dir) = &self.state.output_directory {
                // 显示目录名
                let dir_name = output_dir.display().to_string();
                let response = ui.label(dir_name);
                
                // 悬停时显示完整路径
                let full_path = output_dir.to_string_lossy().to_string();
                response.on_hover_text(full_path);
            } else {
                ui.label("[未设置 - 将使用桌面上的\"BLMM导出\"文件夹]");
            }
            
            if ui.button("选择输出目录").clicked() {
                if let Some(dir) = rfd::FileDialog::new()
                    .set_title("选择输出目录")
                    .pick_folder() {
                        self.state.output_directory = Some(dir.clone());
                        self.config.output_directory = Some(dir.clone());
                        self.state.add_log(&format!("已设置输出目录: {}", dir.display()));
                    }
            }
            
            if ui.button("清除").clicked() {
                self.state.output_directory = None;
                self.config.output_directory = None;
                self.state.add_log("已清除输出目录设置，将使用桌面上的\"BLMM导出\"文件夹");
            }
        });
        
        // 添加默认输出目录说明
        ui.add_space(4.0);
        ui.label("提示: 未设置输出目录时，将默认在桌面上创建\"BLMM导出\"文件夹，并将合并后的MO文件保存到此处。");
        
        ui.separator();
        
        // 常用设置部分
        ui.heading("常用设置");
        
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.state.auto_batch, "自动批处理");
            ui.checkbox(&mut self.state.auto_close, "处理完成后关闭");
        });
        
        ui.checkbox(&mut self.state.show_logs, "显示日志窗口");
        
        // 高级设置部分
        ui.collapsing("高级设置", |ui| {
            // 新增选项: 忽略主MO合并
            ui.checkbox(&mut self.state.ignore_main_mo_entries, "忽略主mo合并")
                .on_hover_text("启用后，语言包中与主MO文件重复的条目将被忽略，保留主MO文件中的原始翻译");
            
            ui.horizontal(|ui| {
                ui.label(format!("线程池: {} 线程", num_cpus::get()));
            });
        });
        
        // 检查配置是否有变更，如果有则保存
        if orig_main_mo_file != self.state.main_mo_file ||
           orig_dark_mode != self.state.dark_mode ||
           orig_auto_batch != self.state.auto_batch ||
           orig_auto_close != self.state.auto_close ||
           orig_show_logs != self.state.show_logs ||
           orig_ignore_main_mo_entries != self.state.ignore_main_mo_entries ||
           orig_theme != self.config.theme {
            // 保存设置到配置文件
            self.config.main_mo_file = self.state.main_mo_file.clone();
            self.config.dark_mode = self.state.dark_mode;
            self.config.auto_batch = self.state.auto_batch;
            self.config.auto_close = self.state.auto_close;
            self.config.show_logs = self.state.show_logs;
            self.config.ignore_main_mo_entries = self.state.ignore_main_mo_entries;
            
            if let Err(e) = self.config.save() {
                self.state.add_log(&format!("无法保存配置: {}", e));
            }
        }
    }

    // 安装新语言包
    fn install_new_mod(&mut self) {
        // 获取或创建MOD缓存目录
        let mods_dir = self.get_or_create_mods_cache_dir();
        if mods_dir.is_none() {
            self.state.add_log("错误: 无法创建语言包缓存目录");
            return;
        }
        
        let mods_dir = mods_dir.unwrap();
        
        // 自动设置mods_directory到固定的缓存目录
        self.state.mods_directory = Some(mods_dir.clone());
        self.config.mods_directory = Some(mods_dir.clone());
        
        // 打开文件选择对话框，允许多选，同时支持PO和CSV文件
        if let Some(files) = rfd::FileDialog::new()
            .add_filter("翻译文件", &["po", "csv"])
            .add_filter("PO文件", &["po"])
            .add_filter("CSV文件", &["csv"])
            .set_title("选择要安装的翻译文件")
            .pick_files() {
                
            let files_count = files.len();
            self.state.add_log(&format!("选择了 {} 个翻译文件准备安装", files_count));
            
            // 记录成功安装的文件数量
            let mut success_count = 0;
            
            // 创建临时缓存目录用于CSV转换
            let cache_dir = if let Some(local_dir) = dirs::data_local_dir() {
                local_dir.join("BLMM").join("cache")
            } else {
                std::env::temp_dir().join("BLMM").join("cache")
            };
            
            // 确保缓存目录存在
            if let Err(e) = std::fs::create_dir_all(&cache_dir) {
                self.state.add_log(&format!("创建缓存目录失败: {}", e));
                return;
            }
            
            // 处理每一个选择的文件
            for file in files {
                // 确定文件类型
                let file_ext = file.extension().unwrap_or_default().to_string_lossy().to_lowercase();
                
                // 对于CSV文件，先转换为PO
                let processed_file = if file_ext == "csv" {
                    self.state.add_log(&format!("检测到CSV文件: {}", file.display()));
                    
                    // 生成临时PO文件
                    let temp_po_path = cache_dir.join(format!("temp_{}.po", SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()));
                    
                    // 转换CSV到PO
                    match CsvConverter::convert_csv_to_po(&file, &temp_po_path) {
                        Ok(_) => {
                            self.state.add_log(&format!("成功将CSV转换为PO: {}", temp_po_path.display()));
                            temp_po_path
                        },
                        Err(e) => {
                            self.state.add_log(&format!("CSV转换为PO失败: {}", e));
                            continue;  // 跳过此文件
                        }
                    }
                } else {
                    file.clone()
                };
                
                // 创建新的MOD信息
                let orig_file_name = file.file_name().unwrap_or_default().to_string_lossy().to_string();
                let mut file_name = processed_file.file_name().unwrap_or_default().to_string_lossy().to_string();
                
                // 如果是从CSV转换的PO，给文件名加上标记
                if file_ext == "csv" {
                    let stem = orig_file_name.strip_suffix(".csv").unwrap_or(&orig_file_name);
                    file_name = format!("{}_from_csv.po", stem);
                }
                
                // 检查是否已存在同名语言包，如果存在则添加"new"后缀
                let mut counter = 0;
                let original_name = file_name.clone();
                let stem = if let Some(pos) = original_name.rfind('.') {
                    &original_name[0..pos]
                } else {
                    &original_name
                };
                let ext = if let Some(pos) = original_name.rfind('.') {
                    &original_name[pos..]
                } else {
                    ""
                };
                
                // 检查名称是否已存在，如果存在则添加"new"后缀
                while self.state.installed_mods.iter().any(|m| m.name == file_name) || mods_dir.join(&file_name).exists() {
                    counter += 1;
                    if counter == 1 {
                        file_name = format!("{}new{}", stem, ext);
                    } else {
                        file_name = format!("{}new{}{}", stem, counter, ext);
                    }
                }
                
                let mut mod_info = ModInfo::default();
                mod_info.name = file_name.clone();
                mod_info.status = ModStatus::Enabled; // 默认为启用状态
                mod_info.install_date = Some(Local::now());
                
                // 如果来自CSV，添加描述
                if file_ext == "csv" {
                    mod_info.description = Some("从CSV转换的PO文件".to_string());
                    mod_info.original_type = Some("CSV".to_string());
                }
                
                // 将PO文件复制到MOD目录
                let target_path = mods_dir.join(&file_name);
                
                // 尝试复制文件
                match std::fs::copy(&processed_file, &target_path) {
                    Ok(_) => {
                        mod_info.path = target_path.clone();
                        
                        // 在配置中保存该mod的启用状态
                        self.config.saved_mods.insert(file_name.clone(), true);
                        
                        // 如果存在原始文件类型信息，创建元数据JSON文件
                        if let Some(orig_type) = &mod_info.original_type {
                            let metadata_path = target_path.with_extension("json");
                            let metadata = serde_json::json!({
                                "name": file_name,
                                "original_type": orig_type,
                                "install_date": chrono::Local::now().to_rfc3339()
                            });
                            
                            // 将元数据写入JSON文件
                            if let Ok(json_str) = serde_json::to_string_pretty(&metadata) {
                                if let Err(e) = std::fs::write(&metadata_path, json_str) {
                                    self.state.add_log(&format!("无法写入元数据文件: {}", e));
                                }
                            }
                        }
                        
                        self.state.installed_mods.push(mod_info);
                        
                        // 标记需要重新合并
                        self.state.needs_remerge = true;
                        
                        // 如果文件名被修改，添加相应日志
                        if file_name != original_name {
                            self.state.add_log(&format!("检测到同名语言包，已重命名为: {}", file_name));
                        }
                        
                        // 显示成功信息，区分CSV和PO
                        if file_ext == "csv" {
                            self.state.add_log(&format!("成功将CSV文件转换并安装为语言包: {}", file_name));
                        } else {
                            self.state.add_log(&format!("成功安装语言包: {}", file_name));
                        }
                        
                        success_count += 1;
                        
                        // 如果是临时文件，安装后删除
                        if file_ext == "csv" {
                            let _ = std::fs::remove_file(&processed_file);
                        }
                    },
                    Err(e) => {
                        let file_display = file.file_name().unwrap_or_default().to_string_lossy();
                        self.state.add_log(&format!("语言包 {} 安装失败: {}", file_display, e));
                        
                        // 如果是临时文件，安装失败也要删除
                        if file_ext == "csv" {
                            let _ = std::fs::remove_file(&processed_file);
                        }
                    }
                }
            }
            
            // 安装完成后更新配置并显示汇总信息
            if success_count > 0 {
                // 保存配置
                self.config.save().ok();
                
                // 如果安装了多个文件，显示汇总信息
                if files_count > 1 {
                    self.state.add_log(&format!("批量安装完成：成功 {}/{}个语言包", success_count, files_count));
                }
            }
        }
    }
    
    // 扫描MOD目录
    fn scan_mods_directory(&mut self) {
        // 获取或创建MOD缓存目录
        let mods_dir = self.get_or_create_mods_cache_dir();
        if mods_dir.is_none() {
            self.state.add_log("错误: 无法创建语言包缓存目录");
            return;
        }
        
        let mods_dir = mods_dir.unwrap();
        
        // 自动设置mods_directory到固定的缓存目录
        self.state.mods_directory = Some(mods_dir.clone());
        self.config.mods_directory = Some(mods_dir.clone());
        
        // 清空当前MOD列表
        self.state.installed_mods.clear();
        
        // 扫描目录下的所有PO文件
        match std::fs::read_dir(&mods_dir) {
            Ok(entries) => {
                let mut found = false;
                
                for entry in entries {
                    if let Ok(entry) = entry {
                        let path = entry.path();
                        
                        // 检查是否为PO文件
                        if path.is_file() && path.extension().map_or(false, |e| e == "po") {
                            found = true;
                            
                            // 创建MOD信息
                            let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                            let mut mod_info = ModInfo::default();
                            mod_info.name = file_name.clone();
                            mod_info.path = path.clone();
                            
                            // 尝试读取同名的json元数据文件
                            let metadata_path = path.with_extension("json");
                            if metadata_path.exists() {
                                if let Ok(meta_content) = std::fs::read_to_string(&metadata_path) {
                                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&meta_content) {
                                        // 尝试获取原始文件类型
                                        if let Some(orig_type) = json.get("original_type").and_then(|v| v.as_str()) {
                                            mod_info.original_type = Some(orig_type.to_string());
                                            if orig_type == "CSV" {
                                                mod_info.description = Some("从CSV转换的PO文件".to_string());
                                            }
                                        }
                                    }
                                }
                            }
                            
                            // 从配置中加载该mod的启用状态
                            if let Some(enabled) = self.config.saved_mods.get(&file_name) {
                                mod_info.status = if *enabled {
                                    ModStatus::Enabled
                                } else {
                                    ModStatus::Disabled
                                };
                            } else {
                                // 如果没有保存的状态，默认为启用
                                mod_info.status = ModStatus::Enabled;
                            }
                            
                            // 获取文件信息
                            if let Ok(metadata) = std::fs::metadata(&mod_info.path) {
                                // 尝试获取安装日期（基于文件创建时间）
                                if let Ok(created) = metadata.created() {
                                    if let Ok(duration) = created.duration_since(UNIX_EPOCH) {
                                        mod_info.install_date = Local.timestamp_opt(duration.as_secs() as i64, 0).single();
                                    }
                                }
                            }
                            
                            // 添加到MOD列表
                            self.state.installed_mods.push(mod_info);
                        }
                    }
                }
                
                if found {
                    self.state.add_log(&format!("扫描完成，发现 {} 个语言包", self.state.installed_mods.len()));
                } else {
                    self.state.add_log("未在目录中找到任何PO语言包");
                }
                
                // 保存配置
                self.config.save().ok();
            },
            Err(e) => {
                self.state.add_log(&format!("扫描语言包目录失败: {}", e));
            }
        }
    }
    
    // 启用MOD
    fn enable_mod(&mut self, index: usize) {
        if index < self.state.installed_mods.len() {
            self.state.installed_mods[index].status = ModStatus::Enabled;
            let mod_name = &self.state.installed_mods[index].name;
            
            // 在配置中保存该mod的启用状态
            self.config.saved_mods.insert(mod_name.clone(), true);
            self.config.save().ok();
            
            // 标记需要重新合并
            self.state.needs_remerge = true;
            
            self.state.add_log(&format!("已启用语言包: {}", mod_name));
        }
    }
    
    // 禁用MOD
    fn disable_mod(&mut self, index: usize) {
        if index < self.state.installed_mods.len() {
            self.state.installed_mods[index].status = ModStatus::Disabled;
            let mod_name = &self.state.installed_mods[index].name;
            
            // 在配置中保存该mod的禁用状态
            self.config.saved_mods.insert(mod_name.clone(), false);
            self.config.save().ok();
            
            // 标记需要重新合并
            self.state.needs_remerge = true;
            
            self.state.add_log(&format!("已禁用语言包: {}", mod_name));
        }
    }
    
    // 卸载MOD
    fn uninstall_mod(&mut self, index: usize) {
        if index < self.state.installed_mods.len() {
            let mod_info = &self.state.installed_mods[index];
            let mod_name = mod_info.name.clone();
            
            // 尝试删除文件
            match std::fs::remove_file(&mod_info.path) {
                Ok(_) => {
                    // 从配置中移除该mod的状态记录
                    self.config.saved_mods.remove(&mod_name);
                    self.config.save().ok();
                    
                    self.state.installed_mods.remove(index);
                    
                    // 标记需要重新合并
                    self.state.needs_remerge = true;
                    
                    self.state.add_log(&format!("已卸载语言包: {}", mod_name));
                },
                Err(e) => {
                    self.state.add_log(&format!("卸载语言包失败: {}", e));
                }
            }
        }
    }

    // 应用退出时保存配置
    fn save_config_on_exit(&mut self) {
        // 确保配置对象包含最新的状态
        self.config.main_mo_file = self.state.main_mo_file.clone();
        self.config.mods_directory = self.state.mods_directory.clone();
        self.config.output_directory = self.state.output_directory.clone();
        
        // 保持向后兼容的dark_mode设置
        self.config.dark_mode = self.state.dark_mode;
        
        self.config.auto_batch = self.state.auto_batch;
        self.config.auto_close = self.state.auto_close;
        self.config.show_logs = self.state.show_logs;
        self.config.ignore_main_mo_entries = self.state.ignore_main_mo_entries;
        
        // 保存配置
        if let Err(e) = self.config.save() {
            self.state.add_log(&format!("退出时保存配置失败: {}", e));
        } else {
            self.state.add_log("配置已保存");
        }
    }

    // 将主MO文件复制到BLMM目录
    fn clone_main_mo_to_blmm(&mut self, original_mo_path: &PathBuf) -> Option<PathBuf> {
        use std::fs;

        // 获取BLMM缓存目录
        let blmm_dir = if let Some(local_dir) = dirs::data_local_dir() {
            local_dir.join("BLMM")
        } else {
            // 如果无法获取系统本地数据目录，使用临时目录
            std::env::temp_dir().join("BLMM")
        };

        // 确保目录存在
        if let Err(e) = fs::create_dir_all(&blmm_dir) {
            self.state.add_log(&format!("创建BLMM目录失败: {}", e));
            return None;
        }

        // 为MO文件创建一个新的路径
        // 不再需要原始文件名，直接使用固定名称
        let blmm_mo_path = blmm_dir.join("base_blender.mo");

        // 复制文件
        match fs::copy(original_mo_path, &blmm_mo_path) {
            Ok(_) => {
                self.state.add_log(&format!("已将主MO文件复制到BLMM目录: {}", blmm_mo_path.display()));
                Some(blmm_mo_path)
            },
            Err(e) => {
                self.state.add_log(&format!("复制主MO文件到BLMM目录失败: {}", e));
                None
            }
        }
    }

    // 处理合并状态更新
    fn process_merge_status(&mut self) {
        // 更新动画计数器
        if self.state.is_merging {
            self.state.merge_progress_anim += 1;
            
            // 添加平滑过渡效果
            // 如果有目标进度，则逐渐接近该进度
            if let Some(target_progress) = self.state.target_merge_progress {
                if (target_progress - self.state.merge_progress).abs() > 0.001 {
                    // 增加插值速率，使进度条更快地接近目标值
                    self.state.merge_progress += (target_progress - self.state.merge_progress) * 0.1;
                } else {
                    // 如果已经非常接近目标，直接设置为目标值
                    self.state.merge_progress = target_progress;
                }
            }
        }
        
        if let Ok(status) = self.merge_rx.try_recv() {
            match status {
                MergeStatus::Started => {
                    self.state.is_merging = true;
                    self.state.merge_progress = 0.0;
                    self.state.target_merge_progress = Some(0.0);
                    self.state.add_log("开始合并PO文件...");
                },
                MergeStatus::Progress(progress) => {
                    // 设置目标进度，而不是直接设置当前进度
                    self.state.target_merge_progress = Some(progress);
                    
                    // 从进度更新日志，确保显示百分比
                    let percent = (progress * 100.0) as i32;
                    self.state.add_log(&format!("合并进度: {}%", percent));
                    
                    // 移除中间停顿的逻辑，让进度条直接平滑过渡到目标值
                    // 不再需要特殊处理99%的情况
                },
                MergeStatus::Completed(path) => {
                    // 先设置进度为100%，再设置合并状态为false
                    self.state.merge_progress = 1.0;
                    self.state.target_merge_progress = Some(1.0);
                    
                    // 添加一个短暂延迟，让用户能看到100%的进度
                    // 在实际应用中，可以使用一个计时器或帧计数器来实现
                    self.state.add_log("合并完成: 100%");
                    
                    // 延迟设置合并状态为false，让用户能看到"合并完成"
                    // 这里我们不立即设置is_merging为false，而是在几帧后设置
                    // 可以添加一个计数器字段来实现
                    self.state.merge_complete_countdown = Some(30); // 30帧后设置为false
                    
                    // 检查是否为 OpenAI 响应（使用 PathBuf 传递文本响应）
                    // 检查是否为 OpenAI 响应（使用 PathBuf 传递文本响应）
                    if path.is_absolute() {
                        // 正常的文件路径，表示合并完成
                        self.state.cached_merged_po = Some(path.clone());
                        self.state.needs_remerge = false;
                        self.state.add_log(&format!("PO文件合并成功，已生成缓存文件: {}", path.display()));
                        self.state.add_log("点击'应用到MO文件'将合并结果应用到主MO文件");
                        
                        // 如果存在已设置的主MO文件，自动应用
                        if self.state.main_mo_file.is_some() {
                            self.state.add_log("自动应用到主MO文件...");
                            if self.state.cached_merged_po.is_some() {
                                self.apply_merged_po_to_mo();
                            }
                        }
                    } else {
                        // 非绝对路径，表示 OpenAI 响应文本
                        let response_text = path.to_string_lossy().to_string();
                        self.state.openai_response = Some(response_text);
                        self.state.openai_is_processing = false;
                        self.state.add_log("收到 OpenAI API 响应");
                    }
                },
                MergeStatus::Failed(error) => {
                    // 检查是否为 OpenAI 错误
                    if self.state.openai_is_processing {
                        self.state.openai_is_processing = false;
                        self.state.openai_last_error = Some(error.clone());
                        self.state.add_log(&format!("OpenAI 请求失败: {}", error));
                    } else {
                        self.state.is_merging = false;
                        self.state.add_log(&format!("合并失败: {}", error));
                    }
                }
            }
        }
    }

    // 专门用于显示帮助信息的函数
    fn show_help_window(&mut self, ctx: &egui::Context) {
        if self.state.show_help {
            egui::Window::new("使用帮助")
                .collapsible(false)
                .min_width(500.0)
                .show(ctx, |ui| {
                    ui.heading("Blender字典合并管理器 By:凌川雪");
                    ui.label("使用帮助");
                    ui.separator();
                    
                    ui.collapsing("基本使用流程", |ui| {
                        ui.add_space(5.0);
                        
                        ui.label("1. 设置 - 选择主MO文件和语言包目录");
                        ui.label("   - 进入设置选项卡，设置Blender的主MO文件");
                        ui.label("   - 设置存放PO语言包的目录");
                        ui.add_space(5.0);
                        
                        ui.label("2. 安装语言包");
                        ui.label("   - 点击「安装语言包」按钮选择PO文件");
                        ui.label("   - 安装后语言包会自动启用");
                        ui.add_space(5.0);
                        
                        ui.label("3. 管理语言包");
                        ui.label("   - 勾选/取消勾选语言包以启用/禁用");
                        ui.label("   - 使用▲▼按钮调整语言包优先级");
                        ui.label("   - 优先级高的语言包翻译会覆盖优先级低的翻译");
                        ui.add_space(5.0);
                        
                        ui.label("4. 应用更改");
                        ui.label("   - 修改语言包状态后点击「重新合并」按钮");
                        ui.label("   - 等待处理完成后，将自动应用到MO文件");
                    });
                    
                    ui.collapsing("高级选项", |ui| {
                        ui.label("- 在设置中可启用「忽略主mo合并」选项，保留原始MO翻译");
                        ui.label("- 通过上下移动语言包调整优先级，高优先级的语言包翻译会覆盖低优先级的");
                        ui.label("- 应用到MO文件后，需要重启Blender才能看到更改效果");
                    });
                    
                    ui.collapsing("故障排除", |ui| {
                        ui.label("如果合并失败:");
                        ui.label("1. 检查主MO文件是否可读写");
                        ui.label("2. 确保语言包是标准的PO格式");
                        ui.label("3. 在日志区查看详细错误信息");
                        ui.label("4. 尝试启用或禁用「忽略主mo合并」选项");
                    });
                    
                    ui.separator();
                    
                    if ui.button("关闭").clicked() {
                        self.state.show_help = false;
                    }
                });
        }
    }

    // 重命名对话框
    fn render_rename_dialog(&mut self, ctx: &egui::Context) {
        if self.state.rename_mod_index.is_some() {
            egui::Window::new("重命名语言包")
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("新名称:");
                        ui.text_edit_singleline(&mut self.state.rename_mod_name);
                    });
                    
                    ui.horizontal(|ui| {
                        if ui.button("确定").clicked() {
                            if let Some(index) = self.state.rename_mod_index {
                                self.rename_mod(index, self.state.rename_mod_name.clone());
                            }
                            self.state.rename_mod_index = None;
                        }
                        
                        if ui.button("取消").clicked() {
                            self.state.rename_mod_index = None;
                        }
                    });
                });
        }
    }
    
    // 重命名语言包
    fn rename_mod(&mut self, index: usize, new_name: String) {
        if index >= self.state.installed_mods.len() || new_name.trim().is_empty() {
            return;
        }
        
        // 获取语言包目录
        let mods_dir = match self.get_or_create_mods_cache_dir() {
            Some(dir) => dir,
            None => {
                self.state.add_log("错误: 无法获取语言包目录");
                return;
            }
        };
        
        let mod_info = &mut self.state.installed_mods[index];
        let old_name = mod_info.name.clone();
        let old_path = mod_info.path.clone();
        
        // 如果新名称与旧名称相同，则不做任何操作
        if old_name == new_name {
            return;
        }
        
        // 确保新文件名以.po结尾
        let new_name_with_ext = if new_name.to_lowercase().ends_with(".po") {
            new_name
        } else {
            format!("{}.po", new_name)
        };
        
        let new_path = mods_dir.join(&new_name_with_ext);
        
        // 尝试重命名文件
        match std::fs::rename(&old_path, &new_path) {
            Ok(_) => {
                // 更新模组信息
                mod_info.name = new_name_with_ext.clone();
                mod_info.path = new_path;
                
                // 更新配置中的状态记录
                if let Some(is_enabled) = self.config.saved_mods.remove(&old_name) {
                    self.config.saved_mods.insert(new_name_with_ext.clone(), is_enabled);
                }
                
                // 保存配置
                self.config.save().ok();
                
                self.state.add_log(&format!("语言包重命名成功: {} -> {}", old_name, new_name_with_ext));
                
                // 标记需要重新合并
                self.state.needs_remerge = true;
            },
            Err(e) => {
                self.state.add_log(&format!("语言包重命名失败: {}", e));
            }
        }
    }

    // 自动定位Blender中文MO文件
    fn auto_locate_blender_mo_file(&mut self) {
        self.state.add_log("正在自动搜索Blender中文MO文件...");
        
        // 常见的Blender安装路径
        let common_paths = vec![
            "C:/Program Files/Blender Foundation",
            "D:/Program Files/Blender Foundation",
            "C:/Program Files (x86)/Blender Foundation",
        ];
        
        // 首先让用户选择Blender主目录
        let selected_blender_dir = rfd::FileDialog::new()
            .set_title("选择Blender安装目录")
            .set_directory(common_paths[0])
            .pick_folder();
            
        if let Some(blender_dir) = selected_blender_dir {
            // 只显示目录名称，避免过长
            let dir_name = blender_dir.file_name()
                .map_or_else(|| "[未知目录]".to_string(),
                          |name| name.to_string_lossy().to_string());
            self.state.add_log(&format!("选择了Blender目录: {}", dir_name));
            
            // 在选定目录中查找版本目录（如Blender 4.3）
            let mut version_dirs = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&blender_dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if path.is_dir() {
                        let dir_name = path.file_name().unwrap_or_default().to_string_lossy();
                        if dir_name.starts_with("Blender ") {
                            version_dirs.push(path.clone());
                            self.state.add_log(&format!("找到Blender版本: {}", dir_name));
                        }
                    }
                }
            }
            
            // 如果找到版本目录，让用户选择
            let selected_version_dir = if !version_dirs.is_empty() {
                // 如果只有一个版本目录，直接使用
                if version_dirs.len() == 1 {
                    Some(version_dirs[0].clone())
                } else {
                    // 创建版本目录名称列表，供用户选择
                    let version_dir_names: Vec<String> = version_dirs.iter()
                        .map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string())
                        .collect();
                        
                    // 让用户从对话框中选择版本
                    let selected_result = rfd::MessageDialog::new()
                        .set_title("选择Blender版本")
                        .set_description(&format!("找到多个Blender版本，请选择一个:\n{}", 
                            version_dir_names.join("\n")))
                        .set_buttons(rfd::MessageButtons::OkCancel)
                        .show();
                        
                    if selected_result {
                        // 如果用户点击确定，让他们选择具体的版本目录
                        rfd::FileDialog::new()
                            .set_title("选择Blender版本目录")
                            .set_directory(&blender_dir)
                            .pick_folder()
                    } else {
                        None
                    }
                }
            } else {
                // 没有找到标准的版本目录，直接使用所选目录
                Some(blender_dir)
            };
            
            // 如果选择了版本目录，继续查找子版本目录（如4.3）
            if let Some(version_dir) = selected_version_dir {
                self.state.add_log(&format!("选择的版本目录: {}", version_dir.display()));
                
                // 在版本目录中查找子版本目录（如4.3）
                let mut subversion_dirs = Vec::new();
                if let Ok(entries) = std::fs::read_dir(&version_dir) {
                    for entry in entries.filter_map(|e| e.ok()) {
                        let path = entry.path();
                        if path.is_dir() {
                            let dir_name = path.file_name().unwrap_or_default().to_string_lossy();
                            // 检查是否为版本号格式（如4.3）
                            if dir_name.chars().any(|c| c.is_digit(10)) && dir_name.contains('.') {
                                subversion_dirs.push(path.clone());
                                self.state.add_log(&format!("找到子版本目录: {}", dir_name));
                            }
                        }
                    }
                }
                
                // 选择要使用的子版本目录
                let target_dir = if !subversion_dirs.is_empty() {
                    // 如果只有一个子版本目录，询问用户是否使用
                    if subversion_dirs.len() == 1 {
                        let result = rfd::MessageDialog::new()
                            .set_title("确认子版本目录")
                            .set_description(&format!("是否使用子版本目录: {}?", 
                                subversion_dirs[0].file_name().unwrap_or_default().to_string_lossy()))
                            .set_buttons(rfd::MessageButtons::YesNo)
                            .show();
                            
                        if result {
                            subversion_dirs[0].clone()
                        } else {
                            version_dir
                        }
                    } else {
                        // 让用户从多个子版本目录中选择
                        let selected_subversion = rfd::FileDialog::new()
                            .set_title("选择Blender子版本目录")
                            .set_directory(&version_dir)
                            .pick_folder();
                            
                        selected_subversion.unwrap_or(version_dir)
                    }
                } else {
                    version_dir
                };
                
                self.state.add_log(&format!("将在目录中搜索MO文件: {}", target_dir.display()));
                
                // 在目标目录中查找MO文件
                let mut found_mo_files = Vec::new();
                
                // 构建可能的语言文件路径
                let mo_paths = vec![
                    target_dir.join("datafiles/locale/zh_HANS/LC_MESSAGES/blender.mo"),
                    target_dir.join("datafiles/locale/zh_CN/LC_MESSAGES/blender.mo"),
                    target_dir.join("locale/zh_HANS/LC_MESSAGES/blender.mo"),
                    target_dir.join("locale/zh_CN/LC_MESSAGES/blender.mo"),
                ];
                
                // 检查每个路径
                for path in mo_paths {
                    if path.exists() && path.is_file() {
                        self.state.add_log(&format!("找到MO文件: {}", path.display()));
                        found_mo_files.push(path);
                    }
                }
                
                // 如果没找到，尝试递归搜索
                if found_mo_files.is_empty() {
                    self.state.add_log(&format!("在标准路径未找到MO文件，尝试递归搜索: {}", target_dir.display()));
                    self.search_mo_files_recursively(&target_dir, &mut found_mo_files);
                }
                
                // 如果找到文件，让用户选择或自动选择第一个
                if !found_mo_files.is_empty() {
                    // 按文件路径排序
                    found_mo_files.sort();
                    
                    // 如果只有一个文件，直接使用它
                    if found_mo_files.len() == 1 {
                        let file_path = found_mo_files[0].clone();
                        let orig_path = file_path.clone(); // 克隆一份用于日志显示
                        // 复制到BLMM目录并使用BLMM目录中的文件
                        if let Some(blmm_path) = self.clone_main_mo_to_blmm(&file_path) {
                            self.state.main_mo_file = Some(blmm_path.clone());
                            self.config.main_mo_file = Some(blmm_path);
                        } else {
                            self.state.main_mo_file = Some(file_path.clone());
                            self.config.main_mo_file = Some(file_path);
                        }
                        self.config.save().ok();
                        self.add_log_with_path("已自动设置唯一找到的MO文件", &orig_path);
                    } else {
                        // 让用户从找到的文件中选择
                        self.state.add_log(&format!("找到 {} 个MO文件，请选择一个:", found_mo_files.len()));
                        
                        if let Some(selected_path) = rfd::FileDialog::new()
                            .set_title("选择Blender中文MO文件")
                            .add_filter("MO文件", &["mo"])
                            .set_directory(found_mo_files[0].parent().unwrap_or(&PathBuf::from("/")))
                            .pick_file() {
                            
                            let orig_path = selected_path.clone(); // 克隆一份用于日志显示
                            // 复制到BLMM目录并使用BLMM目录中的文件
                            if let Some(blmm_path) = self.clone_main_mo_to_blmm(&selected_path) {
                                self.state.main_mo_file = Some(blmm_path.clone());
                                self.config.main_mo_file = Some(blmm_path);
                            } else {
                                self.state.main_mo_file = Some(selected_path.clone());
                                self.config.main_mo_file = Some(selected_path);
                            }
                            self.config.save().ok();
                            self.add_log_with_path("已设置主MO文件", &orig_path);
                        }
                    }
                } else {
                    self.state.add_log("未找到Blender中文MO文件，请手动选择。");
                    
                    // 打开文件选择对话框
                    if let Some(file) = rfd::FileDialog::new()
                        .add_filter("MO文件", &["mo"])
                        .set_title("选择Blender中文MO文件")
                        .set_directory(&target_dir)
                        .pick_file() {
                            // 复制到BLMM目录并使用BLMM目录中的文件
                            let orig_file = file.clone(); // 克隆一份用于日志显示
                            if let Some(blmm_path) = self.clone_main_mo_to_blmm(&file) {
                                self.state.main_mo_file = Some(blmm_path.clone());
                                self.config.main_mo_file = Some(blmm_path);
                            } else {
                                self.state.main_mo_file = Some(file.clone());
                                self.config.main_mo_file = Some(file);
                            }
                            self.config.save().ok();
                            self.add_log_with_path("已设置主MO文件", &orig_file);
                    }
                }
            }
        } else {
            self.state.add_log("未选择Blender目录，操作取消。");
        }
    }
    
    // 递归搜索MO文件
    fn search_mo_files_recursively(&mut self, dir: &PathBuf, found_files: &mut Vec<PathBuf>) {
        // 设置最大深度为8，避免搜索太深导致性能问题
        let max_depth = 8;
        
        for entry in walkdir::WalkDir::new(dir)
            .follow_links(true)
            .max_depth(max_depth)
            .into_iter()
            .filter_map(|e| e.ok()) {
            
            let path = entry.path();
            
            // 检查是否为MO文件
            if path.is_file() && path.extension().map_or(false, |e| e.to_string_lossy().to_lowercase() == "mo") {
                // 检查文件路径是否包含中文相关关键词
                let path_str = path.to_string_lossy().to_lowercase();
                if (path_str.contains("zh_") || path_str.contains("chinese") || 
                    path_str.contains("zh-") || path_str.contains("/zh/") || 
                    path_str.contains("\\zh\\")) && path_str.contains("blender") {
                    
                    // 获取文件名用于日志显示
                    let file_name = path.file_name()
                        .map_or_else(|| "[未知文件]".to_string(), 
                                   |name| name.to_string_lossy().to_string());
                    self.state.add_log(&format!("递归搜索找到MO文件: {}", file_name));
                    found_files.push(path.to_path_buf());
                }
            }
        }
    }

    fn add_log_with_path(&mut self, message: &str, path: &PathBuf) {
        // 提取文件名用于日志显示
        let file_name = path.file_name()
            .map_or_else(|| "[未知文件]".to_string(), 
                      |name| name.to_string_lossy().to_string());
        self.state.add_log(&format!("{}: {}", message, file_name));
    }

    // 导出基础文件（不合并）
    fn export_base_mo_file(&mut self) {
        // 检查是否有主MO文件
        if self.state.main_mo_file.is_none() {
            self.state.add_log("错误: 请先在设置中设置主MO文件");
            return;
        }
        
        let base_mo_file = self.state.main_mo_file.clone().unwrap();
        
        // 检查文件是否存在
        if !base_mo_file.exists() {
            self.state.add_log(&format!("错误: 基础MO文件不存在: {}", base_mo_file.display()));
            return;
        }
        
        // 创建输出MO文件路径 - 使用用户设置的输出目录或桌面上的"BLMM导出"文件夹
        let output_mo_path = if let Some(output_dir) = &self.state.output_directory {
            // 使用用户设置的输出目录
            if let Err(e) = std::fs::create_dir_all(output_dir) {
                self.state.add_log(&format!("创建输出目录失败: {}", e));
                // 如果创建目录失败，回退到桌面上的"BLMM导出"文件夹
                self.create_default_output_directory()
                    .map(|dir| dir.join("blender.mo"))
                    .unwrap_or_else(|| base_mo_file.with_file_name("blender.mo"))
            } else {
                // 使用设置的输出目录
                output_dir.join("blender.mo")
            }
        } else {
            // 未设置输出目录，使用桌面上的"BLMM导出"文件夹
            self.create_default_output_directory()
                .map(|dir| dir.join("blender.mo"))
                .unwrap_or_else(|| {
                    // 如果创建桌面文件夹失败，回退到主MO文件所在目录
                    if let Some(parent) = base_mo_file.parent() {
                        parent.join("blender.mo")
                    } else {
                        base_mo_file.with_file_name("blender.mo")
                    }
                })
        };
        
        // 复制文件
        self.state.add_log(&format!("正在导出基础MO文件到: {}", output_mo_path.display()));
        match std::fs::copy(&base_mo_file, &output_mo_path) {
            Ok(_) => {
                self.state.add_log(&format!("基础MO文件导出成功: {}", output_mo_path.display()));
                
                // 尝试打开输出目录
                if let Some(parent) = output_mo_path.parent() {
                    if let Err(e) = open::that(parent) {
                        self.state.add_log(&format!("尝试打开输出目录失败: {}", e));
                    }
                }
            },
            Err(e) => {
                self.state.add_log(&format!("导出基础MO文件失败: {}", e));
            }
        }
    }

    // 添加合并PO文件方法
    fn merge_po_files(&mut self) {
        // 检查是否有主MO文件
        if self.state.main_mo_file.is_none() {
            self.state.add_log("错误: 请先在设置中设置主MO文件");
            return;
        }
        
        // 检查是否有启用的语言包
        let enabled_mods = self.state.installed_mods.iter()
            .filter(|m| m.status == ModStatus::Enabled)
            .count();
            
        if enabled_mods == 0 && !self.state.needs_remerge {
            self.state.add_log("错误: 没有启用的语言包需要合并");
            return;
        }
        
        // 设置合并状态
        self.state.is_merging = true;
        self.state.merge_progress = 0.0;
        self.state.merge_progress_anim = 0;
        
        // 在线程中执行合并，以避免UI冻结
        let tx = self.merge_tx.clone();
        let po_files: Vec<PathBuf> = self.state.installed_mods.iter()
            .filter(|m| m.status == ModStatus::Enabled)
            .map(|m| m.path.clone())
            .collect();
        let ignore_main = self.state.ignore_main_mo_entries;
        
        self.thread_pool.spawn(move || {
            // 通知开始
            let _ = tx.send(MergeStatus::Started);
            
            // 创建缓存目录
            let cache_dir = if let Some(local_dir) = dirs::data_local_dir() {
                local_dir.join("BLMM").join("cache")
            } else {
                std::env::temp_dir().join("BLMM").join("cache")
            };
            
            if let Err(e) = std::fs::create_dir_all(&cache_dir) {
                let _ = tx.send(MergeStatus::Failed(format!("创建缓存目录失败: {}", e)));
                return;
            }
            
            // 缓存合并PO的路径
            let cached_po_path = cache_dir.join("cached_merged.po");
            
            // 更新进度 - 添加更多的进度点
            let _ = tx.send(MergeStatus::Progress(0.1)); // 10%
            std::thread::sleep(std::time::Duration::from_millis(100));
            
            let _ = tx.send(MergeStatus::Progress(0.2)); // 20%
            std::thread::sleep(std::time::Duration::from_millis(100));
            
            let _ = tx.send(MergeStatus::Progress(0.3)); // 30%
            
            // 合并PO文件
            match po_merger::merge_po_files(&po_files, &cached_po_path, ignore_main) {
                Ok(_) => {
                    // 更新进度 - 添加更多的进度点
                    let _ = tx.send(MergeStatus::Progress(0.5)); // 50%
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    
                    let _ = tx.send(MergeStatus::Progress(0.9)); // 70%
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    
                    let _ = tx.send(MergeStatus::Progress(1.0)); // 100%
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    
                    // 完成
                    let _ = tx.send(MergeStatus::Completed(cached_po_path));
                },
                Err(e) => {
                    let _ = tx.send(MergeStatus::Failed(format!("合并PO文件失败: {}", e)));
                }
            }
        });
    }

    // 渲染 OpenAI 配置和功能页面
    fn render_openai_tab(&mut self, ui: &mut Ui) {
        // 获取主题颜色
        let accent_color = crate::models::ThemeManager::get_accent_color(&self.config.theme);
        let (_success_color, _warning_color, error_color, _info_color) =
            crate::models::ThemeManager::get_status_colors();
        
        ui.heading("OpenAI 翻译助手");
        
        // 启用/禁用 OpenAI 功能
        let mut enable_openai = self.config.enable_openai;
        if ui.checkbox(&mut enable_openai, "启用 OpenAI API").changed() {
            self.config.enable_openai = enable_openai;
            self.config.save().ok();
        }
        
        ui.add_space(10.0);
        
        if !enable_openai {
            ui.label("请先启用 OpenAI API 功能以使用 AI 翻译助手。");
            return;
        }
        
        egui::Grid::new("openai_config_grid")
            .num_columns(2)
            .spacing([10.0, 10.0])
            .striped(true)
            .show(ui, |ui| {
                // API Key 设置
                ui.label("API Key:");
                let mut api_key = self.config.openai_config.api_key.clone();
                let key_response = ui.add(
                    egui::TextEdit::singleline(&mut api_key)
                        .password(true)
                        .hint_text("sk-")
                );
                if key_response.changed() {
                    self.config.openai_config.api_key = api_key;
                    self.config.save().ok();
                }
                ui.end_row();
                
                // API 基础 URL 设置
                ui.label("API 基础 URL:");
                let mut api_base_url = self.config.openai_config.api_base_url.clone();
                let url_response = ui.add(
                    egui::TextEdit::singleline(&mut api_base_url)
                        .hint_text("https://api.openai.com/v1")
                );
                if url_response.changed() {
                    self.config.openai_config.api_base_url = api_base_url;
                    self.config.save().ok();
                }
                ui.end_row();
                
                // 模型选择
                ui.label("AI 模型:");
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_source("openai_model_combobox")
                        .selected_text(&self.config.openai_config.model)
                        .show_ui(ui, |ui| {
                            // 获取所有可用模型（内置 + 自定义）
                            for model in crate::models::get_all_models(&self.config.openai_config) {
                                if ui.selectable_label(
                                    self.config.openai_config.model == model,
                                    model.clone()
                                ).clicked() {
                                    self.config.openai_config.model = model;
                                    self.config.save().ok();
                                }
                            }
                        });
                        
                    // 添加自定义模型按钮
                    if ui.button("添加模型").clicked() {
                        self.state.show_custom_model_dialog = true;
                        self.state.editing_model_index = None;
                        self.state.new_custom_model_name = String::new();
                        self.state.new_custom_model_id = String::new();
                        self.state.new_custom_model_description = String::new();
                    }
                });
                ui.end_row();
                
                // 温度设置
                ui.label("温度:");
                let mut temperature = self.config.openai_config.temperature;
                let temp_response = ui.add(
                    egui::Slider::new(&mut temperature, 0.0..=1.0)
                        .text("温度")
                );
                if temp_response.changed() {
                    self.config.openai_config.temperature = temperature;
                    self.config.save().ok();
                }
                ui.end_row();
                
                // 最大 token 数量
                ui.label("最大 Tokens:");
                let mut max_tokens = self.config.openai_config.max_tokens;
                let tokens_response = ui.add(
                    egui::Slider::new(&mut max_tokens, 100..=8192)
                        .text("最大 Tokens")
                );
                if tokens_response.changed() {
                    self.config.openai_config.max_tokens = max_tokens;
                    self.config.save().ok();
                }
                ui.end_row();
                
                // 系统提示词
                ui.label("系统提示词:");
                ui.end_row();
                
                ui.label(""); // 空标签用于对齐
                let mut system_prompt = self.config.openai_config.system_prompt.clone();
                let prompt_response = ui.add(
                    egui::TextEdit::multiline(&mut system_prompt)
                        .hint_text("输入系统提示词...")
                        .desired_rows(3)
                        .desired_width(400.0)
                );
                if prompt_response.changed() {
                    self.config.openai_config.system_prompt = system_prompt;
                    self.config.save().ok();
                }
                ui.end_row();
            });
        
        // 显示已添加的自定义模型列表
        if !self.config.openai_config.custom_models.is_empty() {
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(5.0);
            ui.heading("自定义模型");
            
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .show(ui, |ui| {
                    egui::Grid::new("custom_models_grid")
                        .num_columns(4)
                        .spacing([10.0, 5.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.strong("显示名称");
                            ui.strong("模型 ID");
                            ui.strong("描述");
                            ui.strong("操作");
                            ui.end_row();
                            
                            // 获取自定义模型副本，以便安全地修改原始列表
                            let models = self.config.openai_config.custom_models.clone();
                            let mut model_to_delete = None;
                            
                            for (i, model) in models.iter().enumerate() {
                                ui.label(&model.name);
                                ui.label(&model.model_id);
                                ui.label(model.description.as_deref().unwrap_or("-"));
                                
                                ui.horizontal(|ui| {
                                    // 编辑按钮
                                    if ui.button("编辑").clicked() {
                                        self.state.show_custom_model_dialog = true;
                                        self.state.editing_model_index = Some(i);
                                        self.state.new_custom_model_name = model.name.clone();
                                        self.state.new_custom_model_id = model.model_id.clone();
                                        self.state.new_custom_model_description = 
                                            model.description.clone().unwrap_or_default();
                                    }
                                    
                                    // 删除按钮
                                    if ui.button("删除").clicked() {
                                        model_to_delete = Some(i);
                                    }
                                });
                                
                                ui.end_row();
                            }
                            
                            // 如果需要删除某个模型
                            if let Some(index) = model_to_delete {
                                self.config.openai_config.custom_models.remove(index);
                                self.config.save().ok();
                            }
                        });
                });
        }
        
        ui.add_space(20.0);
        ui.separator();
        ui.add_space(10.0);
        
        // 测试功能区域
        ui.heading("测试翻译功能");
        
        // 源语言和目标语言选择
        ui.horizontal(|ui| {
            ui.label("源语言:");
            ui.text_edit_singleline(&mut self.state.openai_source_lang);
            
            ui.label("目标语言:");
            ui.text_edit_singleline(&mut self.state.openai_target_lang);
        });
        
        // 测试输入和结果
        ui.add_space(10.0);
        ui.label("输入测试文本:");
        ui.text_edit_multiline(&mut self.state.openai_test_prompt)
            .on_hover_text("输入要翻译的文本");
        
        // 发送按钮
        ui.horizontal(|ui| {
            let send_button = if self.state.openai_is_processing {
                ui.add_enabled(false, egui::Button::new("处理中..."))
            } else {
                ui.add(egui::Button::new(RichText::new("发送请求").color(accent_color)))
            };
            
            if send_button.clicked() && !self.state.openai_is_processing {
                // 创建 OpenAI 客户端并发送请求
                if self.config.openai_config.api_key.is_empty() {
                    self.state.openai_last_error = Some("API Key 不能为空".to_string());
                } else {
                    self.state.openai_is_processing = true;
                    self.state.openai_response = None;
                    self.state.openai_last_error = None;
                    
                    // 克隆需要的数据用于异步处理
                    let openai_config = self.config.openai_config.clone();
                    let prompt = self.state.openai_test_prompt.clone();
                    let source_lang = self.state.openai_source_lang.clone();
                    let target_lang = self.state.openai_target_lang.clone();
                    let tx = self.merge_tx.clone();
                    
                    // 在单独的线程中处理请求
                    self.thread_pool.spawn(move || {
                        // 创建客户端
                        let client = crate::models::OpenAIClient::new(openai_config);
                        
                        // 执行翻译
                        match client.translate(&prompt, &source_lang, &target_lang) {
                            Ok(response) => {
                                // 发送成功响应
                                let _ = tx.send(crate::ui::app::MergeStatus::Completed(
                                    PathBuf::from(response)
                                ));
                            },
                            Err(error) => {
                                // 发送错误
                                let _ = tx.send(crate::ui::app::MergeStatus::Failed(error));
                            }
                        }
                    });
                }
            }
            
            // 显示API状态
            if !self.state.openai_is_processing {
                if let Some(error) = &self.state.openai_last_error {
                    ui.colored_label(error_color, format!("错误: {}", error));
                }
            }
        });
        
        // 显示结果
        ui.add_space(10.0);
        if self.state.openai_is_processing {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("正在等待 OpenAI 响应...");
            });
        } else if let Some(response) = &self.state.openai_response {
            ui.label("翻译结果:");
            let text_style = egui::TextStyle::Body;
            let font_id = ui.style().text_styles.get(&text_style).unwrap().clone();
            let row_height = ui.fonts(|f| f.row_height(&font_id)) + 4.0;
            
            let available_height = ui.available_height() - 50.0;
            let num_rows = (available_height / row_height).max(5.0).min(20.0) as usize;
            
            egui::ScrollArea::vertical()
                .max_height(row_height * num_rows as f32)
                .show(ui, |ui| {
                    let mut response_clone = response.clone();
                    let _response_label = ui.add(
                        egui::TextEdit::multiline(&mut response_clone)
                            .desired_width(ui.available_width())
                            .desired_rows(num_rows)
                            .interactive(false)
                    );
                    
                    // 添加复制按钮
                    if ui.button("复制结果").clicked() {
                        ui.output_mut(|o| o.copied_text = response.clone());
                    }
                });
        }
        
        ui.add_space(10.0);
        ui.separator();
        
        // AI 辅助功能说明
        ui.heading("功能说明");
        ui.label("OpenAI 翻译助手可以帮助您:");
        ui.add_space(5.0);
        ui.label("1. 翻译游戏内容或用户界面文本");
        ui.label("2. 润色和优化已有的翻译");
        ui.label("3. 统一术语和风格");
        ui.add_space(10.0);
        ui.label("注意: 使用此功能需要有效的 OpenAI API Key 并消耗 API 积分。");
    }
    
    // 自定义模型对话框
    fn render_custom_model_dialog(&mut self, ctx: &egui::Context) {
        if self.state.show_custom_model_dialog {
            let is_editing = self.state.editing_model_index.is_some();
            let title = if is_editing { "编辑自定义模型" } else { "添加自定义模型" };
            
            egui::Window::new(title)
                .fixed_size([400.0, 250.0])
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    egui::Grid::new("add_custom_model_grid")
                        .num_columns(2)
                        .spacing([10.0, 10.0])
                        .show(ui, |ui| {
                            ui.label("显示名称:");
                            ui.add(egui::TextEdit::singleline(&mut self.state.new_custom_model_name)
                                .hint_text("例: My GPT-4 Model"));
                            ui.end_row();
                            
                            ui.label("模型 ID:");
                            ui.add(egui::TextEdit::singleline(&mut self.state.new_custom_model_id)
                                .hint_text("例: gpt-4"));
                            ui.end_row();
                            
                            ui.label("描述 (可选):");
                            ui.add(egui::TextEdit::multiline(&mut self.state.new_custom_model_description)
                                .hint_text("模型描述信息")
                                .desired_rows(3));
                            ui.end_row();
                        });
                        
                    ui.add_space(10.0);
                    
                    ui.horizontal(|ui| {
                        if ui.button("取消").clicked() {
                            self.state.show_custom_model_dialog = false;
                        }
                        
                        let save_text = if is_editing { "保存" } else { "添加" };
                        let can_save = !self.state.new_custom_model_name.trim().is_empty() && 
                                       !self.state.new_custom_model_id.trim().is_empty();
                        
                        if ui.add_enabled(can_save, egui::Button::new(save_text)).clicked() {
                            let new_model = crate::models::CustomModel {
                                name: self.state.new_custom_model_name.trim().to_string(),
                                model_id: self.state.new_custom_model_id.trim().to_string(),
                                description: if self.state.new_custom_model_description.trim().is_empty() {
                                    None
                                } else {
                                    Some(self.state.new_custom_model_description.trim().to_string())
                                },
                            };
                            
                            if let Some(index) = self.state.editing_model_index {
                                // 更新现有模型
                                if index < self.config.openai_config.custom_models.len() {
                                    self.config.openai_config.custom_models[index] = new_model;
                                }
                            } else {
                                // 添加新模型
                                self.config.openai_config.custom_models.push(new_model);
                            }
                            
                            // 保存配置
                            self.config.save().ok();
                            
                            // 关闭对话框
                            self.state.show_custom_model_dialog = false;
                        }
                    });
                });
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Process background task results
        self.process_conversion_results();
        
        // Process merge status updates
        self.process_merge_status();
        
        // 处理合并完成倒计时
        if let Some(countdown) = self.state.merge_complete_countdown {
            if countdown > 0 {
                self.state.merge_complete_countdown = Some(countdown - 1);
            } else {
                self.state.is_merging = false;
                self.state.merge_complete_countdown = None;
            }
        }
        
        // 设置主题
        let visuals = crate::models::ThemeManager::get_visuals(&self.config.theme);
        ctx.set_visuals(visuals);
        
        // 为了向后兼容，保持dark_mode标志与主题同步
        self.state.dark_mode = self.config.theme != crate::models::AppTheme::Light;
        
        // 顶部菜单栏
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                // 文件菜单
                ui.menu_button("文件", |ui| {
                    if ui.button("添加语言包").clicked() {
                        self.state.show_mods = true;
                        ui.close_menu();
                    }
                    
                    ui.separator();
                    
                    // 设置选项 - 直接打开语言包管理器的设置标签页
                    if ui.button("设置").clicked() {
                        self.state.show_mods = true;
                        self.state.show_mods_tab = ModsTab::Settings;
                        ui.close_menu();
                    }
                    
                    ui.separator();
                    
                    if ui.button("退出").clicked() {
                        frame.close();
                        // 关闭时保存配置
                        self.save_config_on_exit();
                        ui.close_menu();
                    }
                });
                
                // 工具菜单
                ui.menu_button("工具", |ui| {
                    if ui.button("合并语言包").clicked() {
                        self.merge_po_files();
                        ui.close_menu();
                    }
                    
                    if ui.button("导出基础MO文件").clicked() {
                        self.export_base_mo_file();
                        ui.close_menu();
                    }
                    
                    ui.separator();
                    
                    if ui.button("批量处理").clicked() {
                        self.batch_process();
                        ui.close_menu();
                    }
                });
                
                // 窗口菜单
                ui.menu_button("窗口", |ui| {
                    if ui.button("语言包管理器").clicked() {
                        self.state.show_mods = true;
                        ui.close_menu();
                    }
                });
                
                // 帮助菜单
                ui.menu_button("帮助", |ui| {
                    if ui.button("使用帮助").clicked() {
                        self.state.show_help = true;
                        ui.close_menu();
                    }
                    
                    ui.separator();
                    
                    if ui.button("关于").clicked() {
                        self.state.show_about = true;
                        ui.close_menu();
                    }
                });
            });
        });
        
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.state.show_mods {
                self.render_mods(ui);
            } else {
                self.render_header(ui);
                self.render_operations(ui);
                ui.separator();
                
                if ui.button("语言包管理器").clicked() {
                    self.state.show_mods = true;
                }
                
                ui.separator();
                self.render_logs(ui);
            }
        });
        
        self.render_settings(ctx);
        self.show_help_window(ctx);
        self.render_rename_dialog(ctx);
        self.render_custom_model_dialog(ctx);
    }
    
    // Override the on_exit method to ensure configuration is saved
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_config_on_exit();
    }
}

#[allow(dead_code)]
fn format_system_time(time: SystemTime) -> String {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => {
            let datetime = Local.timestamp_opt(duration.as_secs() as i64, 0).unwrap();
            datetime.format("%Y-%m-%d %H:%M:%S").to_string()
        },
        Err(_) => "Invalid time".to_string()
    }
} 