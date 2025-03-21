use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use chrono::Local;

pub struct CsvConverter;

impl CsvConverter {
    /// 将CSV文件转换为PO文件
    /// 
    /// # Arguments
    /// 
    /// * `input` - CSV文件路径
    /// * `output` - 输出PO文件路径
    /// 
    /// # Returns
    /// 
    /// 成功返回Ok(()), 失败返回带错误信息的Err
    pub fn convert_csv_to_po(input: &Path, output: &Path) -> Result<(), String> {
        // 打开CSV文件
        let file = File::open(input).map_err(|e| format!("无法打开CSV文件: {}", e))?;
        let reader = BufReader::new(file);
        
        // 创建输出PO文件
        let mut output_file = File::create(output).map_err(|e| format!("无法创建PO文件: {}", e))?;
        
        // 生成PO文件头
        write_po_header(&mut output_file)?;
        
        // 读取并处理每一行
        let mut is_first_line = true;
        let mut has_header = false;
        let mut entries_count = 0;
        
        for line in reader.lines() {
            let mut line = line.map_err(|e| format!("读取CSV文件时出错: {}", e))?;
            
            // 处理BOM标记（UTF-8 BOM）
            if is_first_line && line.starts_with('\u{feff}') {
                line = line[3..].to_string();
            }
            
            is_first_line = false;
            
            // 跳过空行
            if line.trim().is_empty() {
                continue;
            }
            
            // 解析CSV行
            let entries = parse_csv_line(&line)?;
            
            // 必须有源文本和目标文本
            if entries.len() < 2 {
                continue;
            }
            
            // 如果是第一行且内容看起来像表头，则跳过
            if !has_header && (entries[0].contains("源语言") || 
                             entries[0].contains("原文") || 
                             entries[0].contains("msgid") || 
                             entries[0].contains("ID") || 
                             entries[1].contains("翻译") || 
                             entries[1].contains("目标") || 
                             entries[1].contains("译文") || 
                             entries[1].contains("msgstr")) {
                has_header = true;
                continue;
            }
            
            // 获取源文本和目标文本
            let msgid = &entries[0];
            let msgstr = &entries[1];
            
            // 跳过空的源文本
            if msgid.trim().is_empty() {
                continue;
            }
            
            // 写入PO条目
            writeln!(output_file, "msgid {}", escape_po_string(msgid))
                .map_err(|e| format!("写入PO文件时出错: {}", e))?;
            writeln!(output_file, "msgstr {}", escape_po_string(msgstr))
                .map_err(|e| format!("写入PO文件时出错: {}", e))?;
            writeln!(output_file).map_err(|e| format!("写入PO文件时出错: {}", e))?;
            
            entries_count += 1;
        }
        
        // 如果没有有效条目，返回错误
        if entries_count == 0 {
            return Err("CSV文件中未找到有效翻译条目".to_string());
        }
        
        Ok(())
    }
}

/// 解析CSV行，支持引号内的逗号和转义引号
fn parse_csv_line(line: &str) -> Result<Vec<String>, String> {
    let mut result = Vec::new();
    let mut current_field = String::new();
    let mut in_quotes = false;
    
    let mut chars = line.chars().peekable();
    
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                // 处理引号
                if in_quotes {
                    // 检查下一个字符是否也是引号（转义）
                    if chars.peek() == Some(&'"') {
                        current_field.push('"');
                        chars.next(); // 跳过下一个引号
                    } else {
                        in_quotes = false;
                    }
                } else {
                    in_quotes = true;
                }
            },
            ',' => {
                if in_quotes {
                    // 如果在引号内，逗号是字段内容的一部分
                    current_field.push(c);
                } else {
                    // 逗号表示字段结束
                    result.push(current_field);
                    current_field = String::new();
                }
            },
            _ => {
                // 普通字符
                current_field.push(c);
            }
        }
    }
    
    // 添加最后一个字段
    result.push(current_field);
    
    // 如果只有一个字段但包含制表符，尝试使用制表符分割
    if result.len() == 1 && result[0].contains('\t') {
        return Ok(result[0].split('\t').map(|s| s.to_string()).collect());
    }
    
    // 确保至少有两个字段
    if result.len() < 2 {
        // 尝试查找其他分隔符
        for sep in &[';', '|'] {
            if line.contains(*sep) {
                return Ok(line.split(*sep).map(|s| s.trim().to_string()).collect());
            }
        }
    }
    
    Ok(result)
}

/// 将字符串转义为PO格式
fn escape_po_string(s: &str) -> String {
    let escaped = s.replace('\\', "\\\\")
                   .replace('\"', "\\\"")
                   .replace('\n', "\\n")
                   .replace('\r', "\\r")
                   .replace('\t', "\\t");
    
    format!("\"{}\"", escaped)
}

/// 写入PO文件头
fn write_po_header(file: &mut File) -> Result<(), String> {
    let now = Local::now();
    let date_str = now.format("%Y-%m-%d %H:%M%z").to_string();
    
    // 编写PO文件头
    writeln!(file, "msgid \"\"").map_err(|e| format!("写入PO文件头时出错: {}", e))?;
    writeln!(file, "msgstr \"\"").map_err(|e| format!("写入PO文件头时出错: {}", e))?;
    writeln!(file, "\"Project-Id-Version: BLMM Converted CSV\\n\"").map_err(|e| format!("写入PO文件头时出错: {}", e))?;
    writeln!(file, "\"POT-Creation-Date: {}\\n\"", date_str).map_err(|e| format!("写入PO文件头时出错: {}", e))?;
    writeln!(file, "\"PO-Revision-Date: {}\\n\"", date_str).map_err(|e| format!("写入PO文件头时出错: {}", e))?;
    writeln!(file, "\"Language: zh_CN\\n\"").map_err(|e| format!("写入PO文件头时出错: {}", e))?;
    writeln!(file, "\"MIME-Version: 1.0\\n\"").map_err(|e| format!("写入PO文件头时出错: {}", e))?;
    writeln!(file, "\"Content-Type: text/plain; charset=UTF-8\\n\"").map_err(|e| format!("写入PO文件头时出错: {}", e))?;
    writeln!(file, "\"Content-Transfer-Encoding: 8bit\\n\"").map_err(|e| format!("写入PO文件头时出错: {}", e))?;
    writeln!(file, "\"Converted-From-CSV: true\\n\"").map_err(|e| format!("写入PO文件头时出错: {}", e))?;
    writeln!(file).map_err(|e| format!("写入PO文件头时出错: {}", e))?;
    
    Ok(())
} 