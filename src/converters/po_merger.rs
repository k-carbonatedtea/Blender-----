use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::sync::{Arc, Mutex};

/// 合并多个PO文件
/// 
/// 合并策略:
/// 1. 第一个文件被视为基础文件，它的头部信息会被保留
/// 2. 优先级高的文件会覆盖优先级低的文件的相同条目（输入文件列表按优先级顺序排列，首个文件优先级最高）
/// 3. 保留所有文件中的唯一msgid
/// 4. 如果设置了ignore_main_entries为true，则不会覆盖第一个文件中已有的msgstr
/// 
/// # 参数
/// * `input_files` - 输入PO文件路径列表，按优先级排序（第一个最高）
/// * `output_file` - 输出PO文件路径
/// * `ignore_main_entries` - 是否保留第一个文件中已有的翻译
/// 
/// # 返回
/// * `Result<(), String>` - 成功或错误信息
pub fn merge_po_files(input_files: &[PathBuf], output_file: impl AsRef<Path>, ignore_main_entries: bool) -> Result<(), String> {
    if input_files.is_empty() {
        return Err("没有提供输入文件".to_string());
    }
    
    // 创建线程安全的HashMap来存储合并后的条目
    // HashMap <msgid, (msgstr, is_translated, is_from_main_file)>
    let entries: Arc<Mutex<HashMap<String, (String, bool, bool)>>> = Arc::new(Mutex::new(HashMap::new()));
    
    // 记录第一个文件的头部信息
    let mut header = String::new();
    let mut has_header = false;
    
    // 首先处理第一个文件，获取头部信息
    {
        let first_file = &input_files[0];
        let file = File::open(first_file).map_err(|e| format!("无法打开文件 {}: {}", first_file.display(), e))?;
        let reader = BufReader::new(file);
        
        let mut is_header = true;
        let mut current_msgid = String::new();
        let mut current_msgstr = String::new();
        
        for line in reader.lines() {
            let line = line.map_err(|e| format!("读取文件时出错: {}", e))?;
            
            if line.trim().is_empty() {
                if !current_msgid.is_empty() {
                    // 如果是头部条目，保存头部信息
                    if is_header && current_msgid == "\"\"" {
                        header = current_msgstr.clone();
                        has_header = true;
                    }
                    
                    let has_translation = !current_msgstr.is_empty() && current_msgstr != "\"\"";
                    
                    // 将第一个文件的所有条目标记为来自主文件
                    if !current_msgid.is_empty() {
                        let mut entries_lock = entries.lock().unwrap();
                        entries_lock.insert(current_msgid.clone(), (current_msgstr.clone(), has_translation, true));
                    }
                    
                    is_header = false;
                    current_msgid = String::new();
                    current_msgstr = String::new();
                }
                continue;
            }
            
            if line.starts_with("msgid ") {
                current_msgid = line["msgid ".len()..].trim().to_string();
            } else if line.starts_with("msgstr ") {
                current_msgstr = line["msgstr ".len()..].trim().to_string();
            }
        }
        
        // 处理文件末尾的最后一个条目
        if !current_msgid.is_empty() {
            let has_translation = !current_msgstr.is_empty() && current_msgstr != "\"\"";
            
            let mut entries_lock = entries.lock().unwrap();
            entries_lock.insert(current_msgid.clone(), (current_msgstr.clone(), has_translation, true));
        }
    }
    
    // 按优先级顺序处理剩余文件
    // 注意：顺序很重要，我们按照输入文件的顺序处理，确保高优先级的文件覆盖低优先级的文件
    for (file_index, file_path) in input_files.iter().enumerate().skip(1) {
        let file = match File::open(file_path) {
            Ok(f) => f,
            Err(e) => {
                return Err(format!("无法打开文件 {}: {}", file_path.display(), e));
            }
        };
        
        let reader = BufReader::new(file);
        let mut current_msgid = String::new();
        let mut current_msgstr = String::new();
        let mut in_msgid = false;
        let mut in_msgstr = false;
        
        for line_result in reader.lines() {
            let line = match line_result {
                Ok(l) => l,
                Err(e) => {
                    return Err(format!("读取文件时出错: {}", e));
                }
            };
            
            let trimmed = line.trim();
            
            if trimmed.is_empty() {
                if !current_msgid.is_empty() {
                    let has_translation = !current_msgstr.is_empty() && current_msgstr != "\"\"";
                    
                    // 更新条目，同时考虑优先级和是否要忽略主文件中已有的翻译
                    if has_translation {
                        let mut entries_lock = entries.lock().unwrap();
                        match entries_lock.get(&current_msgid) {
                            Some((_, _, true)) if ignore_main_entries => {
                                // 如果设置了忽略主文件中的条目，且该条目来自主文件，则不更新
                                // 什么都不做，保留原有的翻译
                            },
                            _ => {
                                // 基于优先级的覆盖规则：
                                // 1. 低优先级的翻译不会覆盖高优先级的翻译
                                // 2. 如果当前处理的是较高优先级的文件，则直接替换
                                entries_lock.insert(current_msgid.clone(), (current_msgstr.clone(), has_translation, file_index == 0));
                            }
                        }
                    } else {
                        // 如果没有翻译，只有在条目不存在时才添加
                        let mut entries_lock = entries.lock().unwrap();
                        if !entries_lock.contains_key(&current_msgid) {
                            entries_lock.insert(current_msgid.clone(), (current_msgstr.clone(), false, file_index == 0));
                        }
                    }
                    
                    current_msgid = String::new();
                    current_msgstr = String::new();
                    in_msgid = false;
                    in_msgstr = false;
                }
                continue;
            }
            
            if trimmed.starts_with("msgid ") {
                current_msgid = trimmed["msgid ".len()..].to_string();
                in_msgid = true;
                in_msgstr = false;
            } else if trimmed.starts_with("msgstr ") {
                current_msgstr = trimmed["msgstr ".len()..].to_string();
                in_msgid = false;
                in_msgstr = true;
            } else if in_msgid {
                current_msgid = format!("{}\n{}", current_msgid, trimmed);
            } else if in_msgstr {
                current_msgstr = format!("{}\n{}", current_msgstr, trimmed);
            }
        }
        
        // 处理文件末尾可能的最后一个条目
        if !current_msgid.is_empty() {
            let has_translation = !current_msgstr.is_empty() && current_msgstr != "\"\"";
            
            if has_translation {
                let mut entries_lock = entries.lock().unwrap();
                match entries_lock.get(&current_msgid) {
                    Some((_, _, true)) if ignore_main_entries => {
                        // 如果设置了忽略主文件中的条目，且该条目来自主文件，则不更新
                        // 什么都不做，保留原有的翻译
                    },
                    _ => {
                        // 基于优先级的覆盖规则
                        entries_lock.insert(current_msgid.clone(), (current_msgstr.clone(), has_translation, file_index == 0));
                    }
                }
            } else {
                // 如果没有翻译，只有在条目不存在时才添加
                let mut entries_lock = entries.lock().unwrap();
                if !entries_lock.contains_key(&current_msgid) {
                    entries_lock.insert(current_msgid.clone(), (current_msgstr.clone(), false, file_index == 0));
                }
            }
        }
    }
    
    // 写入合并后的文件
    let mut output = File::create(&output_file).map_err(|e| format!("无法创建输出文件: {}", e))?;
    
    // 写入头部信息
    if has_header {
        writeln!(output, "msgid \"\"").map_err(|e| format!("写入文件时出错: {}", e))?;
        writeln!(output, "{}", header).map_err(|e| format!("写入文件时出错: {}", e))?;
        writeln!(output).map_err(|e| format!("写入文件时出错: {}", e))?;
    }
    
    // 写入所有条目
    let entries_lock = entries.lock().unwrap();
    for (msgid, (msgstr, _, _)) in entries_lock.iter() {
        if msgid == "\"\"" {
            continue; // 跳过头部条目，已单独处理
        }
        
        writeln!(output, "msgid {}", msgid).map_err(|e| format!("写入文件时出错: {}", e))?;
        writeln!(output, "msgstr {}", msgstr).map_err(|e| format!("写入文件时出错: {}", e))?;
        writeln!(output).map_err(|e| format!("写入文件时出错: {}", e))?;
    }
    
    Ok(())
} 