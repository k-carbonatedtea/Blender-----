use std::fs::File;
use std::io::{Read, Write, BufWriter};
use std::path::Path;
use rayon::prelude::*;

pub struct MoConverter;

impl MoConverter {
    /// 将MO文件转换为PO文件
    /// 
    /// # Arguments
    /// 
    /// * `input` - MO文件路径
    /// * `output` - 输出PO文件路径
    /// 
    /// # Returns
    /// 
    /// 成功返回Ok(()), 失败返回带错误信息的Err
    pub fn convert_mo_to_po(
        input: &Path, 
        output: &Path
    ) -> Result<(), String> {
        // 读取MO文件
        let mut buffer = Vec::new();
        let mut file = File::open(input).map_err(|e| format!("无法打开MO文件: {}", e))?;
        file.read_to_end(&mut buffer).map_err(|e| format!("无法读取MO文件内容: {}", e))?;
        
        // 解析 .mo 头部
        if buffer.len() < 20 {
            return Err("MO文件格式不正确或文件太小".to_string());
        }
        
        // 检查Magic Number (0x950412DE for little-endian)
        let magic = u32::from_le_bytes(buffer[0..4].try_into().unwrap());
        if magic != 0x9504_12DE {
            return Err(format!("MO文件魔数不正确: {:X}, 应为: 950412DE", magic));
        }
        
        let num_strings = u32::from_le_bytes(buffer[8..12].try_into().unwrap());
        let original_table_offset = u32::from_le_bytes(buffer[12..16].try_into().unwrap());
        let translation_table_offset = u32::from_le_bytes(buffer[16..20].try_into().unwrap());

        let file = File::create(output).map_err(|e| format!("无法创建PO输出文件: {}", e))?;
        let mut writer = BufWriter::new(file);

        // 使用Rayon并行处理所有条目
        let entries = (0..num_strings).into_par_iter().map(|i| {
            let orig_offset = original_table_offset + i * 8;
            let trans_offset = translation_table_offset + i * 8;
            
            if orig_offset as usize + 8 > buffer.len() || trans_offset as usize + 8 > buffer.len() {
                return Err(format!("MO文件格式错误: 偏移量超出文件大小"));
            }
            
            let orig_len = u32::from_le_bytes(buffer[orig_offset as usize..orig_offset as usize + 4].try_into().unwrap());
            let orig_str_offset = u32::from_le_bytes(buffer[orig_offset as usize + 4..orig_offset as usize + 8].try_into().unwrap());
            
            let trans_len = u32::from_le_bytes(buffer[trans_offset as usize..trans_offset as usize + 4].try_into().unwrap());
            let trans_str_offset = u32::from_le_bytes(buffer[trans_offset as usize + 4..trans_offset as usize + 8].try_into().unwrap());
            
            if (orig_str_offset + orig_len) as usize > buffer.len() || (trans_str_offset + trans_len) as usize > buffer.len() {
                return Err(format!("MO文件格式错误: 字符串偏移量超出文件大小"));
            }
            
            let orig = match String::from_utf8(buffer[orig_str_offset as usize..(orig_str_offset + orig_len) as usize].to_vec()) {
                Ok(s) => s,
                Err(_) => return Err(format!("MO文件包含无效的UTF-8字符串")),
            };
            
            let (msgctxt, orig_text) = if let Some(idx) = orig.find('\x04') {
                let (ctx, text) = orig.split_at(idx);
                (Some(ctx.to_string()), text[1..].to_string())
            } else {
                (None, orig)
            };
            
            let trans = match String::from_utf8(buffer[trans_str_offset as usize..(trans_str_offset + trans_len) as usize].to_vec()) {
                Ok(s) => s,
                Err(_) => return Err(format!("MO文件包含无效的UTF-8字符串")),
            };
            
            Ok(MoEntry { msgctxt, orig_text: orig_text, trans_text: trans })
        }).collect::<Result<Vec<_>, String>>()?;
        
        // 首先处理头部信息
        let mut has_header = false;
        for entry in &entries {
            if entry.orig_text.is_empty() {
                // 写入PO文件头
                writeln!(writer, "msgid \"\"").map_err(|e| format!("写入PO文件失败: {}", e))?;
                writeln!(writer, "msgstr \"\"").map_err(|e| format!("写入PO文件失败: {}", e))?;
                
                // 处理头部信息
                for line in entry.trans_text.lines() {
                    let escaped = Self::escape_po_string(line);
                    writeln!(writer, "\"{}\\n\"", escaped).map_err(|e| format!("写入PO文件失败: {}", e))?;
                }
                
                writeln!(writer).map_err(|e| format!("写入PO文件失败: {}", e))?;
                
                has_header = true;
                break;
            }
        }
        
        // 如果没有头部，创建一个标准头部
        if !has_header {
            writeln!(writer, "msgid \"\"").map_err(|e| format!("写入PO文件失败: {}", e))?;
            writeln!(writer, "msgstr \"\"").map_err(|e| format!("写入PO文件失败: {}", e))?;
            writeln!(writer, "\"Content-Type: text/plain; charset=UTF-8\\n\"").map_err(|e| format!("写入PO文件失败: {}", e))?;
            writeln!(writer, "\"Content-Transfer-Encoding: 8bit\\n\"").map_err(|e| format!("写入PO文件失败: {}", e))?;
            writeln!(writer, "\"Plural-Forms: nplurals=2; plural=(n != 1);\\n\"").map_err(|e| format!("写入PO文件失败: {}", e))?;
            writeln!(writer).map_err(|e| format!("写入PO文件失败: {}", e))?;
        }
        
        // 跳过空条目，写入其他所有条目
        for entry in entries {
            if entry.orig_text.is_empty() {
                continue; // 已经处理过头部了
            }
            
            // 写入msgctxt(如果存在)
            if let Some(ctx) = &entry.msgctxt {
                Self::write_po_string(&mut writer, "msgctxt", ctx)?;
            }
            
            // 写入msgid
            Self::write_po_string(&mut writer, "msgid", &entry.orig_text)?;
            
            // 写入msgstr
            Self::write_po_string(&mut writer, "msgstr", &entry.trans_text)?;
            
            writeln!(writer).map_err(|e| format!("写入PO文件失败: {}", e))?;
        }
        
        Ok(())
    }
    
    /// 写入PO格式的字符串
    fn write_po_string<W: Write>(writer: &mut W, prefix: &str, content: &str) -> Result<(), String> {
        let escaped = Self::escape_po_string(content);
        
        if content.contains('\n') || content.len() > 80 {
            // 长字符串或多行文本使用空引号行格式
            writeln!(writer, "{} \"\"", prefix).map_err(|e| format!("写入PO文件失败: {}", e))?;
            
            for line in escaped.lines() {
                writeln!(writer, "\"{}\\n\"", line).map_err(|e| format!("写入PO文件失败: {}", e))?;
            }
        } else {
            // 短字符串直接写在一行
            writeln!(writer, "{} \"{}\"", prefix, escaped).map_err(|e| format!("写入PO文件失败: {}", e))?;
        }
        
        Ok(())
    }
    
    /// 转义PO文件中的字符串
    /// 
    /// # Arguments
    /// 
    /// * `s` - 要转义的字符串
    /// 
    /// # Returns
    /// 
    /// 转义后的字符串
    fn escape_po_string(s: &str) -> String {
        s.replace('\\', "\\\\")
         .replace('\"', "\\\"")
         .replace('\r', "\\r")
         .replace('\t', "\\t")
         // 注意我们不替换'\n'，因为多行处理已经单独处理了换行
    }
}

// MO条目结构体，用于并行处理
struct MoEntry {
    msgctxt: Option<String>,
    orig_text: String,
    trans_text: String,
}

/// 命令行工具入口点
/// 
/// 运行方式: cargo run --bin mo2po <输入.mo文件> <输出.po文件>
#[allow(dead_code)]
fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    
    if args.len() < 3 {
        println!("用法: {} <输入.mo文件> <输出.po文件>", args[0]);
        return Ok(());
    }
    
    let input_path = Path::new(&args[1]);
    let output_path = Path::new(&args[2]);
    
    match MoConverter::convert_mo_to_po(input_path, output_path) {
        Ok(()) => println!("转换完成: {}", output_path.display()),
        Err(e) => eprintln!("转换失败: {}", e),
    }
    
    Ok(())
}
