use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use rayon::prelude::*;
use std::sync::{Arc, Mutex};

pub struct PoConverter;

impl PoConverter {
    /// 将PO文件转换为MO文件
    /// 
    /// # Arguments
    /// 
    /// * `input` - PO文件路径
    /// * `output` - 输出MO文件路径
    /// 
    /// # Returns
    /// 
    /// 成功返回Ok(()), 失败返回带错误信息的Err
    pub fn convert_po_to_mo(input: &Path, output: &Path) -> Result<(), String> {
        // 解析PO文件，获取所有翻译条目
        let entries = Self::parse_po_file(input)?;
        
        // 排序条目 (原始文本)
        let mut sorted_entries: Vec<_> = entries.values().collect();
        sorted_entries.sort_by(|a, b| a.msgid.cmp(&b.msgid));
        
        // 创建输出文件
        let mut file = File::create(output).map_err(|e| format!("无法创建输出文件: {}", e))?;
        
        // 构建MO文件
        Self::write_mo_file(&mut file, sorted_entries)?;
        
        Ok(())
    }
    
    /// 解析PO文件内容，提取所有翻译条目
    fn parse_po_file(input: &Path) -> Result<HashMap<String, PoEntry>, String> {
        // 读取文件内容
        let file = File::open(input).map_err(|e| format!("无法打开输入文件: {}", e))?;
        let reader = BufReader::new(file);
        let lines: Vec<String> = reader.lines().collect::<Result<Vec<_>, io::Error>>()
            .map_err(|e| format!("读取PO文件时出错: {}", e))?;
        
        // 使用线程安全的HashMap收集所有条目
        let entries = Arc::new(Mutex::new(HashMap::new()));
        
        // 按块处理文件，提高并行性能
        let chunks: Vec<_> = lines.chunks(100).collect();
        
        // 首先处理头部信息
        let mut header = PoEntry {
            msgid: String::new(),
            msgstr: String::new(),
        };
        
        chunks.into_par_iter().for_each(|chunk| {
            let mut current_entry = PoEntry::default();
            let mut reading_msgid = false;
            let mut reading_msgstr = false;
            
            for line in chunk {
                let line = line.trim();
                
                if line.is_empty() || line.starts_with('#') {
                    if !current_entry.msgid.is_empty() || (!current_entry.msgid.is_empty() && !current_entry.msgstr.is_empty()) {
                        let mut entries_lock = entries.lock().unwrap();
                        entries_lock.insert(current_entry.msgid.clone(), current_entry.clone());
                    }
                    current_entry = PoEntry::default();
                    reading_msgid = false;
                    reading_msgstr = false;
                    continue;
                }
                
                if line.starts_with("msgid ") {
                    reading_msgid = true;
                    reading_msgstr = false;
                    let content = Self::extract_content(line, "msgid ");
                    current_entry.msgid = content;
                } else if line.starts_with("msgstr ") {
                    reading_msgid = false;
                    reading_msgstr = true;
                    let content = Self::extract_content(line, "msgstr ");
                    current_entry.msgstr = content;
                } else if line.starts_with("\"") && line.ends_with("\"") {
                    let content = Self::extract_string_line(line);
                    if reading_msgid {
                        current_entry.msgid.push_str(&content);
                    } else if reading_msgstr {
                        current_entry.msgstr.push_str(&content);
                    }
                }
            }
            
            if !current_entry.msgid.is_empty() || (!current_entry.msgid.is_empty() && !current_entry.msgstr.is_empty()) {
                let mut entries_lock = entries.lock().unwrap();
                entries_lock.insert(current_entry.msgid.clone(), current_entry);
            }
        });
        
        let result = Arc::try_unwrap(entries).unwrap().into_inner().unwrap();
        
        // 确保有PO头部信息
        if !result.contains_key("") {
            let mut result_with_header = HashMap::new();
            result_with_header.insert(String::new(), PoEntry {
                msgid: String::new(),
                msgstr: "Content-Type: text/plain; charset=UTF-8\nContent-Transfer-Encoding: 8bit\n".to_string(),
            });
            
            for (k, v) in result {
                result_with_header.insert(k, v);
            }
            
            Ok(result_with_header)
        } else {
            Ok(result)
        }
    }
    
    /// 从行中提取内容
    fn extract_content(line: &str, prefix: &str) -> String {
        let content = line.trim_start_matches(prefix).trim();
        if content.starts_with('"') && content.ends_with('"') && content.len() >= 2 {
            Self::unescape_po_string(&content[1..content.len()-1])
        } else {
            String::new()
        }
    }
    
    /// 从字符串行提取内容
    fn extract_string_line(line: &str) -> String {
        if line.starts_with('"') && line.ends_with('"') && line.len() >= 2 {
            Self::unescape_po_string(&line[1..line.len()-1])
        } else {
            String::new()
        }
    }
    
    /// 反转义PO文件中的字符串
    fn unescape_po_string(s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        let mut chars = s.chars().peekable();
        
        while let Some(c) = chars.next() {
            if c == '\\' && chars.peek().is_some() {
                match chars.next().unwrap() {
                    'n' => result.push('\n'),
                    'r' => result.push('\r'),
                    't' => result.push('\t'),
                    '\\' => result.push('\\'),
                    '"' => result.push('"'),
                    c => {
                        result.push('\\');
                        result.push(c);
                    }
                }
            } else {
                result.push(c);
            }
        }
        
        result
    }
    
    /// 写入MO文件
    fn write_mo_file<W: Write>(writer: &mut W, entries: Vec<&PoEntry>) -> Result<(), String> {
        // MO文件格式常量
        const MAGIC_NUMBER: u32 = 0x9504_12DE; // Little endian
        const MO_HEADER_SIZE: u32 = 28;
        
        // 计算表的大小和位置
        let num_strings = entries.len() as u32;
        let original_table_offset = MO_HEADER_SIZE;
        let translation_table_offset = original_table_offset + num_strings * 8;
        
        // 预先计算字符串偏移
        let string_start_offset = translation_table_offset + num_strings * 8;
        
        // 预先计算所有字符串在文件中的位置
        let mut string_offsets = Vec::with_capacity(entries.len() * 2);
        let mut current_offset = string_start_offset;
        let mut string_data = Vec::new();
        
        // 首先确保空字符串(头信息)在最前面
        let mut sorted_entries = entries;
        sorted_entries.sort_by(|a, b| {
            if a.msgid.is_empty() {
                std::cmp::Ordering::Less
            } else if b.msgid.is_empty() {
                std::cmp::Ordering::Greater
            } else {
                a.msgid.cmp(&b.msgid)
            }
        });
        
        for entry in &sorted_entries {
            // 原始文本: msgid
            let msgid_bytes = entry.msgid.as_bytes();
            string_offsets.push((msgid_bytes.len() as u32, current_offset));
            string_data.extend_from_slice(msgid_bytes);
            string_data.push(0); // Null terminator
            current_offset += msgid_bytes.len() as u32 + 1;
            
            // 翻译文本: msgstr
            let msgstr_bytes = entry.msgstr.as_bytes();
            string_offsets.push((msgstr_bytes.len() as u32, current_offset));
            string_data.extend_from_slice(msgstr_bytes);
            string_data.push(0); // Null terminator
            current_offset += msgstr_bytes.len() as u32 + 1;
        }
        
        // 写入MO文件头
        writer.write_all(&MAGIC_NUMBER.to_le_bytes()).map_err(|e| format!("写入MO文件头失败: {}", e))?;
        writer.write_all(&0u32.to_le_bytes()).map_err(|e| format!("写入MO文件头失败: {}", e))?; // File format revision
        writer.write_all(&num_strings.to_le_bytes()).map_err(|e| format!("写入MO文件头失败: {}", e))?;
        writer.write_all(&original_table_offset.to_le_bytes()).map_err(|e| format!("写入MO文件头失败: {}", e))?;
        writer.write_all(&translation_table_offset.to_le_bytes()).map_err(|e| format!("写入MO文件头失败: {}", e))?;
        writer.write_all(&0u32.to_le_bytes()).map_err(|e| format!("写入MO文件头失败: {}", e))?; // Size of hashing table
        writer.write_all(&0u32.to_le_bytes()).map_err(|e| format!("写入MO文件头失败: {}", e))?; // Offset of hashing table
        
        // 写入原始文本表 (msgid 偏移表)
        for i in 0..num_strings as usize {
            let (length, offset) = string_offsets[i * 2];
            writer.write_all(&length.to_le_bytes()).map_err(|e| format!("写入原始文本表失败: {}", e))?;
            writer.write_all(&offset.to_le_bytes()).map_err(|e| format!("写入原始文本表失败: {}", e))?;
        }
        
        // 写入翻译文本表 (msgstr 偏移表)
        for i in 0..num_strings as usize {
            let (length, offset) = string_offsets[i * 2 + 1];
            writer.write_all(&length.to_le_bytes()).map_err(|e| format!("写入翻译文本表失败: {}", e))?;
            writer.write_all(&offset.to_le_bytes()).map_err(|e| format!("写入翻译文本表失败: {}", e))?;
        }
        
        // 写入所有字符串数据
        writer.write_all(&string_data).map_err(|e| format!("写入字符串数据失败: {}", e))?;
        
        Ok(())
    }
}

#[derive(Debug, Default, Clone)]
struct PoEntry {
    msgid: String,
    msgstr: String,
}

/// 命令行工具入口点
/// 
/// 运行方式: cargo run --bin po2mo <输入.po文件> <输出.mo文件>
#[allow(dead_code)]
fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    
    if args.len() < 3 {
        println!("用法: {} <输入.po文件> <输出.mo文件>", args[0]);
        return Ok(());
    }
    
    let input_path = Path::new(&args[1]);
    let output_path = Path::new(&args[2]);
    
    match PoConverter::convert_po_to_mo(input_path, output_path) {
        Ok(()) => println!("转换完成: {}", output_path.display()),
        Err(e) => eprintln!("转换失败: {}", e),
    }
    
    Ok(())
} 