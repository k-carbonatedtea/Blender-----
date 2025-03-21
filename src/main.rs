mod models;
mod converters;
mod ui;

use eframe::egui;
use std::env;
use std::fs;
use std::path::Path;
use std::process;

#[cfg(target_os = "windows")]
fn is_admin() -> bool {
    is_elevated::is_elevated()
}

#[cfg(not(target_os = "windows"))]
fn is_admin() -> bool {
    // 非Windows系统，暂时返回true（或实现其他平台的检测逻辑）
    true
}

#[cfg(target_os = "windows")]
fn restart_as_admin() -> Result<(), &'static str> {
    use std::ptr::{null, null_mut};
    use std::os::windows::ffi::OsStrExt;
    use std::ffi::OsStr;
    use winapi::um::shellapi::ShellExecuteW;
    use winapi::um::winuser::SW_SHOW;
    
    let exe_path = env::current_exe()
        .map_err(|_| "无法获取当前可执行文件路径")?;
    
    let exe_path_wide: Vec<u16> = OsStr::new(exe_path.to_str().unwrap_or(""))
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
        
    // 获取命令行参数
    let args: Vec<String> = env::args().skip(1).collect(); // 跳过可执行文件名
    let args_str = args.join(" ");
    let args_wide: Vec<u16> = OsStr::new(&args_str)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    
    // 将"runas"操作转换为宽字符
    let operation: Vec<u16> = OsStr::new("runas")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    
    let result = unsafe {
        ShellExecuteW(
            null_mut(),
            operation.as_ptr(),
            exe_path_wide.as_ptr(),
            args_wide.as_ptr(),
            null(),
            SW_SHOW
        )
    };
    
    // 检查结果，如果大于32则表示成功启动
    if result as isize > 32 {
        Ok(())
    } else {
        Err("以管理员权限重启应用程序失败")
    }
}

use crate::converters::mo_converter::MoConverter;

// 将字体文件嵌入到二进制文件中
const EMBEDDED_MSYH_TTF: &[u8] = include_bytes!("../Fonts/msyh.ttf");

// 将图标数据嵌入到二进制文件中
const EMBEDDED_ICON_DATA: &[u8] = include_bytes!("../assets/icon.png");

fn main() -> eframe::Result<()> {
    // 检查是否以管理员权限运行
    #[cfg(target_os = "windows")]
    if !is_admin() {
        match restart_as_admin() {
            Ok(_) => {
                // 重启成功，退出当前进程
                std::process::exit(0);
            }
            Err(e) => {
                // 重启失败，显示错误并继续运行
                eprintln!("警告: {}", e);
                eprintln!("程序将继续以普通权限运行，可能无法修改系统文件夹内容。");
            }
        }
    }
    
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
        icon_data: load_icon(),
        ..Default::default()
    };
    
    eframe::run_native(
        "Blender 字典合并管理器 0.2.0 By:凌川雪",
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

// 加载应用图标
fn load_icon() -> Option<eframe::IconData> {
    match image::load_from_memory(EMBEDDED_ICON_DATA) {
        Ok(image) => {
            let image = image.to_rgba8();
            let (width, height) = image.dimensions();
            let rgba = image.into_raw();
            Some(eframe::IconData {
                rgba,
                width,
                height,
            })
        }
        Err(_) => None
    }
}
