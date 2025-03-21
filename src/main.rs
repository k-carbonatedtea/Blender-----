mod models;
mod converters;
mod ui;

use eframe::egui;
use std::sync::Arc;
use std::env;
use std::fs;
use std::path::Path;
use std::process;

use crate::converters::mo_converter::MoConverter;

// 将字体文件嵌入到二进制文件中
const EMBEDDED_MSYH_TTF: &[u8] = include_bytes!("../Fonts/msyh.ttf");

fn main() -> eframe::Result<()> {
    // 检查命令行参数，允许直接转换
    let args: Vec<String> = env::args().collect();
    
    // 如果提供了命令行参数，尝试直接转换
    if args.len() >= 3 && args[1] == "--convert" {
        if args.len() < 4 {
            println!("用法: {} --convert input.mo output.po", args[0]);
            process::exit(1);
        }
        
        let input_path = Path::new(&args[2]);
        let output_path = Path::new(&args[3]);
        
        println!("正在转换文件: {} -> {}", input_path.display(), output_path.display());
        
        match MoConverter::convert_mo_to_po(input_path, output_path) {
            Ok(_) => {
                println!("转换成功!");
                
                // 显示结果文件大小
                if let Ok(metadata) = fs::metadata(output_path) {
                    let size_kb = metadata.len() / 1024;
                    println!("生成的PO文件大小: {} KB", size_kb);
                }
                
                process::exit(0);
            }
            Err(e) => {
                println!("转换失败: {}", e);
                process::exit(1);
            }
        }
    }
    
    // 否则启动GUI
    let native_options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(800.0, 600.0)),
        ..Default::default()
    };
    
    eframe::run_native(
        "Blender 字典合并管理器 0.1.0",
        native_options,
        Box::new(|cc| {
            // 使用嵌入式字体数据
            let font_data = EMBEDDED_MSYH_TTF.to_vec(); // 使用嵌入的字体数据
            
            let mut fonts = egui::FontDefinitions::default();
            
            // 添加中文字体
            fonts.font_data.insert(
                "msyh".to_owned(),
                egui::FontData::from_owned(font_data),
            );
            
            // 将中文字体放在proportional字体列表的第一位
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "msyh".to_owned());
            
            // 将中文字体也添加到等宽字体列表
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("msyh".to_owned());
            
            // 加载字体
            cc.egui_ctx.set_fonts(fonts);
            
            Box::new(ui::App::new())
        }),
    )
}
