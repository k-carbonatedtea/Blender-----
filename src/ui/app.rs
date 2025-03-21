use eframe::egui;
use egui::{Color32, RichText, Ui};
use std::path::PathBuf;
use std::sync::mpsc::{self, channel, Receiver, Sender};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH, Duration, Instant};
use chrono::prelude::*;
use std::sync::{Arc, Mutex};
use rayon::ThreadPoolBuilder;
use rfd::FileDialog;

use crate::models::{AppState, ConversionType, FileOperation, AppConfig, ConversionStatus, ModStatus, ModInfo, ModsTab};
use crate::converters::mo_converter::MoConverter;
use crate::converters::po_converter::PoConverter;
use crate::converters::po_merger;

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
        let config = AppConfig::load();
        
        // 创建应用状态并从配置中设置值
        let mut state = AppState::default();
        state.main_mo_file = config.main_mo_file.clone();
        state.mods_directory = config.mods_directory.clone();
        state.dark_mode = config.dark_mode;
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
        ui.heading("Blender字典合并管理器 By:凌川雪");
        ui.label("快速将语言包PO文件转换并合并到MO文件中");
        
        ui.add_space(10.0);
        
        // 检查是否有文件被拖入UI的代码已移除
        // 下面的代码已被注释掉，因为已移除拖拽功能
        /*
        // 检查是否有文件被拖入UI
        if !ui.ctx().input(|i| i.raw.dropped_files.is_empty()) {
            let dropped_files = ui.ctx().input(|i| i.raw.dropped_files.clone());
            self.handle_dropped_files(dropped_files);
        }
        */
    }
    
    fn render_operations(&mut self, ui: &mut Ui) {
        ui.heading("文件列表");
        
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
                            ui.label(input.display().to_string());
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
                            ui.label(output.display().to_string());
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
                                ui.label(RichText::new("完成").color(Color32::GREEN));
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
            if ui.button("添加MO→PO任务").clicked() {
                self.open_specific_file_dialog(ConversionType::MoToPo);
            }
            
            if ui.button("添加PO→MO任务").clicked() {
                self.open_specific_file_dialog(ConversionType::PoToMo);
            }
            
            if ui.button("批量处理").clicked() {
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
    
    fn render_settings(&mut self, ctx: &egui::Context) {
        if self.state.show_settings {
            egui::Window::new("设置")
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.checkbox(&mut self.state.dark_mode, "深色模式");
        
        ui.separator();
        
                    // 添加MO文件设置
                    ui.heading("翻译文件设置");
                    ui.label("主MO文件 (用于合并PO语言包):");
                    
                    ui.horizontal(|ui| {
                        if let Some(mo_file) = &self.state.main_mo_file {
                            ui.label(mo_file.display().to_string());
                        } else {
                            ui.label("[未设置]");
                        }
                        
                        if ui.button("选择MO文件").clicked() {
                            if let Some(file) = rfd::FileDialog::new()
                                .add_filter("MO文件", &["mo"])
                                .set_title("选择主MO文件")
                                .pick_file() {
                                    self.state.main_mo_file = Some(file.clone());
                                    self.state.add_log(&format!("已设置主MO文件: {}", file.display()));
                                }
                        }
                    });
                    
                    ui.label("MOD安装目录 (存放PO语言包):");
                    
                    ui.horizontal(|ui| {
                        if let Some(dir) = &self.state.mods_directory {
                            ui.label(dir.display().to_string());
                        } else {
                            ui.label("[未设置]");
                        }
                        
                        if ui.button("选择目录").clicked() {
                            if let Some(dir) = rfd::FileDialog::new()
                                .set_title("选择MOD安装目录")
                                .pick_folder() {
                                    self.state.mods_directory = Some(dir.clone());
                                    self.state.add_log(&format!("已设置MOD安装目录: {}", dir.display()));
                                }
                            }
                        });
                    
                    ui.separator();
                    
                    if ui.button("关闭").clicked() {
                        self.state.show_settings = false;
                    }
                });
        }
        
        // 添加关于窗口的实现
        if self.state.show_about {
            egui::Window::new("关于")
                .collapsible(false)
                .min_width(500.0)
                .show(ctx, |ui| {
                    ui.heading("Blender字典合并管理器 By:凌川雪");
                    ui.label("版本: 1.0.0");
                    ui.separator();
                    
                    // 移除关于窗口中的重复帮助内容
                    ui.label("本软件可以帮助您轻松管理Blender的翻译字典，支持多语言包的安装、优先级调整与合并。");
                    ui.label("点击上方菜单栏中的「帮助 → 使用帮助」获取详细的使用说明。");
                    ui.label("本软件仅供个人使用，禁止用于商业用途。");
                    ui.label("本软件使用 eframe 和 egui 开发，感谢 eframe 和 egui 的开发者们。");
                    ui.label("本软件的开发初衷是方便个人管理Blender的翻译字典，并进行合并。");
                    ui.label("BUG反馈: QQ:2875285430。");
                    
                    // 替换普通标签为可点击的超链接
                    ui.horizontal(|ui| {
                        ui.label("GITHUB:");
                        ui.hyperlink_to("https://github.com/k-carbonatedtea", "https://github.com/k-carbonatedtea");
                    });
                    
                    ui.separator();
                    
                    if ui.button("关闭").clicked() {
                        self.state.show_about = false;
                    }
                });
        }
    }
    
    // 打开文件选择对话框
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
        // Top menu bar
        ui.horizontal(|ui| {
            if ui.selectable_label(self.state.show_mods_tab == ModsTab::Mods, "语言包").clicked() {
                self.state.show_mods_tab = ModsTab::Mods;
            }
            if ui.selectable_label(self.state.show_mods_tab == ModsTab::Package, "仓库").clicked() {
                self.state.show_mods_tab = ModsTab::Package;
            }
            if ui.selectable_label(self.state.show_mods_tab == ModsTab::Settings, "设置").clicked() {
                self.state.show_mods_tab = ModsTab::Settings;
            }
        });

        ui.separator();

        match self.state.show_mods_tab {
            ModsTab::Mods => self.render_mods_list(ui),
            ModsTab::Package => self.render_package_tab(ui),
            ModsTab::Settings => self.render_mod_settings(ui),
        }
    }

    fn render_mods_list(&mut self, ui: &mut Ui) {
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

            // 添加"安装语言包"按钮
            if ui.button("安装语言包").clicked() {
                self.install_new_mod();
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let enabled_count = self.state.installed_mods.iter().filter(|m| m.status == ModStatus::Enabled).count();
                ui.label(format!("{} 语言包 / {} 已启用", self.state.installed_mods.len(), enabled_count));
                
                // 当有启用的语言包时或需要重新合并时显示合并按钮
                if enabled_count > 0 || self.state.needs_remerge {
                    // 如果需要重新合并，显示"重新合并"按钮并使用不同颜色
                    ui.push_id("remerge_button", |ui| {
                        // 如果正在合并中，显示进度动画
                        if self.state.is_merging {
                            let text = format!("合并中{}", ".".repeat(((self.state.merge_progress_anim / 10) % 4) as usize));
                            ui.add(egui::ProgressBar::new(self.state.merge_progress).text(text));
                        } else {
                            let button_text = if self.state.needs_remerge {
                                RichText::new("重新合并").color(Color32::YELLOW)
                            } else {
                                RichText::new("应用到MO文件")
                            };
                            
                            if ui.button(button_text).clicked() {
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
                                    
                                    // 更新进度 25%
                                    let _ = tx.send(MergeStatus::Progress(0.25));
                                    
                                    // 合并PO文件
                                    match po_merger::merge_po_files(&po_files, &cached_po_path, ignore_main) {
                                        Ok(_) => {
                                            // 更新进度 75%
                                            let _ = tx.send(MergeStatus::Progress(0.75));
                                            
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
                                ui.colored_label(text_color, &mod_info.name);

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
                                    ui.label(mod_info.description.as_deref().unwrap_or("语言包")); // Category
                                });
                            });

                            // Highlight when hovered
                            if row_response.response.hovered() {
                                row_response.response.clone().highlight();
                            }

                            // Context menu
                            row_response.response.context_menu(|ui| {
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
        // 先检查是否已经设置了mods目录
        if let Some(dir) = &self.state.mods_directory {
            // 确保目录存在
            if let Err(e) = std::fs::create_dir_all(dir) {
                eprintln!("创建已有的语言包目录失败: {}", e);
                return None;
            }
            return Some(dir.clone());
        }

        // 使用 AppData\Local\BLMM 目录
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
        
        // Create output MO file path
        let output_mo_path = if let Some(parent) = main_mo_file.parent() {
            let file_stem = main_mo_file.file_stem().unwrap_or_default();
            let file_name = format!("{}_merged.mo", file_stem.to_string_lossy());
            parent.join(file_name)
        } else {
            cache_dir.join("merged.mo")
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
                                self.state.add_log(&format!("合并完成! 新MO文件: {}", output_mo_path.display()));
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

    // Restore the refresh_mods_list function
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
        ui.heading("设置");
        
        // 保存原始配置值，以检测更改
        let orig_main_mo_file = self.state.main_mo_file.clone();
        let orig_mods_directory = self.state.mods_directory.clone();
        let orig_dark_mode = self.state.dark_mode;
        let orig_auto_batch = self.state.auto_batch;
        let orig_auto_close = self.state.auto_close;
        let orig_show_logs = self.state.show_logs;
        let orig_ignore_main_mo_entries = self.state.ignore_main_mo_entries;
        
        ui.horizontal(|ui| {
            ui.label("主MO文件路径:");
            
            if let Some(mo_file) = &self.state.main_mo_file {
                ui.label(mo_file.display().to_string());
            } else {
                ui.label("[未设置]");
            }
            
            if ui.button("浏览").clicked() {
                if let Some(file) = rfd::FileDialog::new()
                    .add_filter("MO文件", &["mo"])
                    .set_title("选择主MO文件")
                    .pick_file() {
                        self.state.main_mo_file = Some(file.clone());
                        self.state.add_log(&format!("设置主MO文件: {}", file.display()));
                    }
            }
        });
        
        ui.horizontal(|ui| {
            ui.label("语言包目录:");
            
            if let Some(dir) = &self.state.mods_directory {
                ui.label(dir.display().to_string());
            } else {
                ui.label("[未设置]");
            }
            
            if ui.button("浏览").clicked() {
                if let Some(dir) = rfd::FileDialog::new()
                    .set_title("选择语言包目录")
                    .pick_folder() {
                        self.state.mods_directory = Some(dir.clone());
                        self.state.add_log(&format!("设置语言包目录: {}", dir.display()));
                        
                        // Automatically scan the directory for mods
                        self.scan_mods_directory();
                    }
            }
        });
        
        ui.separator();
        
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.state.auto_batch, "自动批处理");
            ui.checkbox(&mut self.state.auto_close, "处理完成后关闭");
        });
        
        ui.checkbox(&mut self.state.show_logs, "显示日志窗口");
        
        ui.collapsing("高级设置", |ui| {
                    ui.checkbox(&mut self.state.dark_mode, "深色模式");
            
            // 新增选项: 忽略主MO合并
            ui.checkbox(&mut self.state.ignore_main_mo_entries, "忽略主mo合并")
                .on_hover_text("启用后，语言包中与主MO文件重复的条目将被忽略，保留主MO文件中的原始翻译");
            
            ui.horizontal(|ui| {
                ui.label(format!("线程池: {} 线程", num_cpus::get()));
            });
        });
        
        // 检查配置是否有变更，如果有则保存
        if orig_main_mo_file != self.state.main_mo_file ||
           orig_mods_directory != self.state.mods_directory ||
           orig_dark_mode != self.state.dark_mode ||
           orig_auto_batch != self.state.auto_batch ||
           orig_auto_close != self.state.auto_close ||
           orig_show_logs != self.state.show_logs ||
           orig_ignore_main_mo_entries != self.state.ignore_main_mo_entries
        {
            // 更新配置对象
            self.config.main_mo_file = self.state.main_mo_file.clone();
            self.config.mods_directory = self.state.mods_directory.clone();
            self.config.dark_mode = self.state.dark_mode;
            self.config.auto_batch = self.state.auto_batch;
            self.config.auto_close = self.state.auto_close;
            self.config.show_logs = self.state.show_logs;
            self.config.ignore_main_mo_entries = self.state.ignore_main_mo_entries;
            
            // 保存配置到文件
            if let Err(e) = self.config.save() {
                self.state.add_log(&format!("保存配置失败: {}", e));
            } else {
                self.state.add_log("配置已保存");
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
        
        // 打开文件选择对话框
        if let Some(file) = rfd::FileDialog::new()
            .add_filter("PO文件", &["po"])
            .set_title("选择要安装的PO语言包")
            .pick_file() {
                
                // 创建新的MOD信息
                let file_name = file.file_name().unwrap_or_default().to_string_lossy().to_string();
                let mut mod_info = ModInfo::default();
                mod_info.name = file_name.clone();
                mod_info.status = ModStatus::Enabled; // 默认为启用状态
                mod_info.install_date = Some(Local::now());
                
                // 将PO文件复制到MOD目录
                let target_path = mods_dir.join(&file_name);
                
                // 尝试复制文件
                match std::fs::copy(&file, &target_path) {
                    Ok(_) => {
                        mod_info.path = target_path;
                        
                        // 在配置中保存该mod的启用状态
                        self.config.saved_mods.insert(file_name.clone(), true);
                        
                        self.state.installed_mods.push(mod_info);
                        
                        // 标记需要重新合并
                        self.state.needs_remerge = true;
                        
                        self.state.add_log(&format!("成功安装语言包: {}", file_name));
                        
                        // 自动更新mods_directory到缓存目录
                        if self.state.mods_directory.is_none() {
                            self.state.mods_directory = Some(mods_dir.clone());
                            self.config.mods_directory = Some(mods_dir);
                        }
                        
                        // 保存配置
                        self.config.save().ok();
                        self.state.add_log("已自动设置语言包目录");
                    },
                    Err(e) => {
                        self.state.add_log(&format!("安装语言包失败: {}", e));
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
                            mod_info.path = path;
                            
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
                
                // 自动更新mods_directory到缓存目录
                if self.state.mods_directory.is_none() {
                    self.state.mods_directory = Some(mods_dir.clone());
                    self.config.mods_directory = Some(mods_dir);
                    self.config.save().ok();
                }
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
        self.config.dark_mode = self.state.dark_mode;
        self.config.auto_batch = self.state.auto_batch;
        self.config.auto_close = self.state.auto_close;
        self.config.show_logs = self.state.show_logs;
        self.config.ignore_main_mo_entries = self.state.ignore_main_mo_entries;
        
        // 保存配置
        if let Err(e) = self.config.save() {
            self.state.add_log(&format!("退出时保存配置失败: {}", e));
        }
    }

    // 在process_conversion_results方法后添加新的方法处理合并进度
    fn process_merge_status(&mut self) {
        // 更新动画计数器
        if self.state.is_merging {
            self.state.merge_progress_anim += 1;
        }
        
        // 检查是否有来自合并线程的消息
        if let Ok(status) = self.merge_rx.try_recv() {
            match status {
                MergeStatus::Started => {
                    self.state.add_log("开始合并PO文件...");
                },
                MergeStatus::Progress(progress) => {
                    self.state.merge_progress = progress;
                    self.state.add_log(&format!("合并进度: {}%", (progress * 100.0) as i32));
                },
                MergeStatus::Completed(cached_path) => {
                    self.state.is_merging = false;
                    self.state.merge_progress = 1.0;
                    self.state.cached_merged_po = Some(cached_path.clone());
                    self.state.needs_remerge = false;
                    self.state.add_log(&format!("PO文件合并成功，已生成缓存文件: {}", cached_path.display()));
                    self.state.add_log("点击'应用到MO文件'将合并结果应用到主MO文件");
                    
                    // 如果缓存文件可用，则自动应用到MO文件
                    if self.state.cached_merged_po.is_some() {
                        self.apply_merged_po_to_mo();
                    }
                },
                MergeStatus::Failed(error) => {
                    self.state.is_merging = false;
                    self.state.add_log(&format!("合并失败: {}", error));
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
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Process background task results
        self.process_conversion_results();
        
        // Process merge status updates
        self.process_merge_status();
        
        // Set theme
        if self.state.dark_mode {
            ctx.set_visuals(egui::Visuals::dark());
        } else {
            ctx.set_visuals(egui::Visuals::light());
        }
        
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("文件", |ui| {
                    if ui.button("退出").clicked() {
                        frame.close();
                        // 关闭时保存配置
                        self.save_config_on_exit();
                    }
                });
                
                ui.menu_button("工具", |ui| {
                    if ui.button("转换 MO→PO").clicked() {
                        self.open_specific_file_dialog(ConversionType::MoToPo);
                        ui.close_menu();
                    }
                    if ui.button("转换 PO→MO").clicked() {
                        self.open_specific_file_dialog(ConversionType::PoToMo);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("批量处理").clicked() {
                        self.batch_process();
                        ui.close_menu();
                    }
                });
                
                ui.menu_button("窗口", |ui| {
                    if ui.button("语言包管理器").clicked() {
                        self.state.show_mods = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("设置").clicked() {
                        self.state.show_settings = true;
                        ui.close_menu();
                    }
                });
                
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
    }
    
    // Override the on_exit method to ensure configuration is saved
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_config_on_exit();
    }
}

// 辅助函数：格式化时间
fn format_system_time(time: SystemTime) -> String {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => {
            // 使用chrono格式化时间
            let datetime = Local.timestamp_opt(duration.as_secs() as i64, 0)
                .single()
                .unwrap_or_else(|| Local::now());
            datetime.format("%Y-%m-%d %H:%M:%S").to_string()
        },
        Err(_) => "无效时间".to_string()
    }
} 