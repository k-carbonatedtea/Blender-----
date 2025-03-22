use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::sync::{Arc, Mutex};
use std::error::Error;

// PO条目结构
#[derive(Debug, Clone)]
struct PoEntry {
    msgctxt: Option<String>,  // 消息上下文
    msgid: String,           // 原文
    msgstr: String,         // 译文
    comments: Vec<String>,  // 注释
    is_fuzzy: bool,        // 是否为模糊翻译
    line_number: usize,    // 在文件中的行号
    source_file: String,   // 来源文件
}

impl Default for PoEntry {
    fn default() -> Self {
        Self {
            msgctxt: None,
            msgid: String::new(),
            msgstr: String::new(),
            comments: Vec::new(),
            is_fuzzy: false,
            line_number: 0,
            source_file: String::new(),
        }
    }
}

// 解析状态
#[derive(PartialEq)]
enum ParseState {
    None,
    Comment,
    MsgCtxt,
    MsgId,
    MsgStr,
}

/// 合并多个PO文件
/// 
/// # 参数
/// * `input_files` - 输入PO文件路径列表,按优先级排序(第一个最高)
/// * `output_file` - 输出PO文件路径
/// * `ignore_main_entries` - 是否保留第一个文件中已有的翻译
/// 
/// # 返回
/// * `Result<(), String>` - 成功或错误信息
pub fn merge_po_files(input_files: &[PathBuf], output_file: impl AsRef<Path>, ignore_main_entries: bool) -> Result<(), String> {
    if input_files.is_empty() {
        return Err("没有提供输入文件".to_string());
    }

    // 获取第一个文件的名称
    let first_file_name = input_files[0].file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    // 用于存储所有条目的HashMap
    // key: (msgctxt, msgid), value: PoEntry
    let entries: Arc<Mutex<HashMap<(Option<String>, String), PoEntry>>> = Arc::new(Mutex::new(HashMap::new()));
    
    // 记录第一个文件的头部信息
    let mut header = String::new();
    let mut has_header = false;

    // 处理所有输入文件
    for (file_index, file_path) in input_files.iter().enumerate() {
        let file = File::open(file_path).map_err(|e| format!("无法打开文件 {}: {}", file_path.display(), e))?;
        let reader = BufReader::new(file);
        let mut current_entry = PoEntry::default();
        let mut state = ParseState::None;
        let mut line_number = 0;

        current_entry.source_file = file_path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        for line in reader.lines() {
            line_number += 1;
            let line = line.map_err(|e| format!("读取文件时出错: {}", e))?;
            let trimmed = line.trim();

            // 处理空行 - 表示一个条目的结束
            if trimmed.is_empty() {
                if !current_entry.msgid.is_empty() {
                    // 处理头部信息
                    if file_index == 0 && current_entry.msgid == "\"\"" && !has_header {
                        header = current_entry.msgstr.clone();
                        has_header = true;
                    } else {
                        // 存储条目
                        store_entry(&entries, current_entry.clone(), file_index, ignore_main_entries, &first_file_name)?;
                    }
                }
                current_entry = PoEntry {
                    source_file: file_path.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                    line_number,
                    ..Default::default()
                };
                state = ParseState::None;
                continue;
            }

            // 处理各种类型的行
            match trimmed {
                s if s.starts_with('#') => {
                    state = ParseState::Comment;
                    current_entry.comments.push(s.to_string());
                    if s.contains("fuzzy") {
                        current_entry.is_fuzzy = true;
                    }
                },
                s if s.starts_with("msgctxt ") => {
                    state = ParseState::MsgCtxt;
                    current_entry.msgctxt = Some(parse_po_string(&s["msgctxt ".len()..])?);
                },
                s if s.starts_with("msgid ") => {
                    state = ParseState::MsgId;
                    current_entry.msgid = parse_po_string(&s["msgid ".len()..])?;
                },
                s if s.starts_with("msgstr ") => {
                    state = ParseState::MsgStr;
                    current_entry.msgstr = parse_po_string(&s["msgstr ".len()..])?;
                },
                s if s.starts_with('"') => {
                    // 继续前一个字符串
                    let content = parse_po_string(s)?;
                    match state {
                        ParseState::MsgCtxt => {
                            if let Some(ref mut ctx) = current_entry.msgctxt {
                                ctx.push_str(&content);
                            }
                        },
                        ParseState::MsgId => current_entry.msgid.push_str(&content),
                        ParseState::MsgStr => current_entry.msgstr.push_str(&content),
                        _ => return Err(format!("文件 {} 第 {} 行出现意外的字符串继续", file_path.display(), line_number)),
                    }
                },
                _ => return Err(format!("文件 {} 第 {} 行格式错误: {}", file_path.display(), line_number, trimmed)),
            }
        }

        // 处理文件最后一个条目
        if !current_entry.msgid.is_empty() {
            store_entry(&entries, current_entry, file_index, ignore_main_entries, &first_file_name)?;
        }
    }

    // 写入合并后的文件
    let mut output = File::create(&output_file)
        .map_err(|e| format!("无法创建输出文件: {}", e))?;

    // 写入头部信息
    if has_header {
        writeln!(output, "msgid \"\"").map_err(|e| format!("写入文件时出错: {}", e))?;
        writeln!(output, "{}", header).map_err(|e| format!("写入文件时出错: {}", e))?;
        writeln!(output).map_err(|e| format!("写入文件时出错: {}", e))?;
    }

    // 获取所有条目并排序
    let entries_lock = entries.lock().unwrap();
    let mut sorted_entries: Vec<_> = entries_lock.values().collect();
    sorted_entries.sort_by(|a, b| {
        if a.msgid.is_empty() { return std::cmp::Ordering::Less; }
        if b.msgid.is_empty() { return std::cmp::Ordering::Greater; }
        a.msgid.cmp(&b.msgid)
    });

    // 在sorted_entries排序之前添加调试信息
    println!("Total entries before sorting: {}", sorted_entries.len());
    println!("Entries with msgctxt:");
    for entry in &sorted_entries {
        if let Some(ref ctx) = entry.msgctxt {
            println!("msgctxt: {}, msgid: {}", ctx, entry.msgid);
        }
    }

    // 写入所有条目
    for entry in sorted_entries {
        // 写入注释
        for comment in &entry.comments {
            writeln!(output, "{}", comment).map_err(|e| format!("写入文件时出错: {}", e))?;
        }

        // 写入msgctxt(如果有)
        if let Some(ref ctx) = entry.msgctxt {
            write_po_string(&mut output, "msgctxt", ctx)?;
        }

        // 写入msgid
        write_po_string(&mut output, "msgid", &entry.msgid)?;

        // 写入msgstr
        write_po_string(&mut output, "msgstr", &entry.msgstr)?;

        // 条目之间的空行
        writeln!(output).map_err(|e| format!("写入文件时出错: {}", e))?;
    }

    // 验证输出文件
    validate_po_file(&output_file)?;

    Ok(())
}

// 存储PO条目
fn store_entry(
    entries: &Arc<Mutex<HashMap<(Option<String>, String), PoEntry>>>,
    entry: PoEntry,
    file_index: usize,
    ignore_main_entries: bool,
    first_file_name: &str,
) -> Result<(), String> {
    let mut entries = entries.lock().unwrap();
    let key = (entry.msgctxt.clone(), entry.msgid.clone());

    println!("Processing entry - msgid: {}, msgctxt: {:?}", 
             entry.msgid, entry.msgctxt);

    match entries.get(&key) {
        Some(existing) => {
            println!("Found existing entry with same key");
            // 如果设置了ignore_main_entries且现有条目来自第一个文件,则保留现有翻译
            if ignore_main_entries && existing.source_file == first_file_name {
                return Ok(());
            }

            // 根据优先级决定是否覆盖
            if file_index == 0 || !existing.is_fuzzy {
                entries.insert(key, entry);
            }
        },
        None => {
            println!("Adding new entry");
            entries.insert(key, entry);
        }
    }

    Ok(())
}

// 解析PO字符串
fn parse_po_string(s: &str) -> Result<String, String> {
    if !s.starts_with('"') || !s.ends_with('"') {
        return Err(format!("无效的PO字符串格式: {}", s));
    }

    let content = &s[1..s.len()-1];
    Ok(unescape_po_string(content))
}

// 转义PO字符串
fn escape_po_string(s: &str) -> String {
    s.replace('\\', "\\\\")
     .replace('\"', "\\\"")
     .replace('\n', "\\n")
     .replace('\r', "\\r")
     .replace('\t', "\\t")
}

// 反转义PO字符串
fn unescape_po_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('\\') => result.push('\\'),
                Some('\"') => result.push('\"'),
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some(x) => result.push(x),
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }
    result
}

// 写入PO字符串
fn write_po_string(output: &mut File, key: &str, value: &str) -> Result<(), String> {
    if value.contains('\n') {
        // 多行字符串
        writeln!(output, "{} \"\"", key)
            .map_err(|e| format!("写入文件时出错: {}", e))?;
        for line in value.split('\n') {
            writeln!(output, "\"{}\\n\"", escape_po_string(line))
                .map_err(|e| format!("写入文件时出错: {}", e))?;
        }
    } else {
        // 单行字符串
        writeln!(output, "{} \"{}\"", key, escape_po_string(value))
            .map_err(|e| format!("写入文件时出错: {}", e))?;
    }
    Ok(())
}

// 验证PO文件
fn validate_po_file(file_path: impl AsRef<Path>) -> Result<(), String> {
    let file = File::open(file_path.as_ref())
        .map_err(|e| format!("无法打开文件进行验证: {}", e))?;
    let reader = BufReader::new(file);
    let mut line_number = 0;
    let mut in_entry = false;
    let mut has_msgid = false;
    let mut has_msgstr = false;

    for line in reader.lines() {
        line_number += 1;
        let line = line.map_err(|e| format!("验证时读取文件出错: {}", e))?;
        let trimmed = line.trim();

        if trimmed.is_empty() {
            if in_entry {
                if !has_msgid || !has_msgstr {
                    return Err(format!("第 {} 行: 不完整的PO条目", line_number));
                }
                in_entry = false;
                has_msgid = false;
                has_msgstr = false;
            }
            continue;
        }

        if trimmed.starts_with("msgid ") {
            in_entry = true;
            has_msgid = true;
        } else if trimmed.starts_with("msgstr ") {
            if !has_msgid {
                return Err(format!("第 {} 行: msgstr前缺少msgid", line_number));
            }
            has_msgstr = true;
        }
    }

    // 检查最后一个条目
    if in_entry && (!has_msgid || !has_msgstr) {
        return Err("文件末尾存在不完整的PO条目".to_string());
    }

    Ok(())
} 