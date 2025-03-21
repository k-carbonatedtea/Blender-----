fn main() {
    // 告诉Cargo如果字体文件改变了，则重新编译
    println!("cargo:rerun-if-changed=Fonts/msyh.ttf");
    
    // 如果图标文件改变，重新编译
    println!("cargo:rerun-if-changed=assets/icon.svg");
    println!("cargo:rerun-if-changed=assets/icon.png");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=winres/app.rc");
    println!("cargo:rerun-if-changed=winres/app.manifest");
    
    // 注意: 字体文件现在已经通过include_bytes!嵌入到可执行文件中
    // 不再需要复制字体文件到输出目录
    
    // 检测是否在Windows平台上，仅在Windows上应用资源
    #[cfg(target_os = "windows")]
    {
        let mut res = winres::WindowsResource::new();
        
        // 设置管理员权限
        res.set_manifest(r#"
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
<trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
        <requestedPrivileges>
            <requestedExecutionLevel level="requireAdministrator" uiAccess="false" />
        </requestedPrivileges>
    </security>
</trustInfo>
</assembly>
"#);
        
        // 设置资源脚本文件，该文件包含图标设置
        let current_dir = std::env::current_dir().unwrap_or_default();
        let rc_path = current_dir.join("winres").join("app.rc");
        
        if rc_path.exists() {
            println!("cargo:warning=使用资源脚本: {}", rc_path.display());
            res.set_resource_file(&rc_path.to_string_lossy());
        } else {
            println!("cargo:warning=未找到资源脚本: {}", rc_path.display());
            
            // 如果没有资源脚本，尝试直接设置图标
            let ico_path = current_dir.join("assets").join("icon.ico");
            if ico_path.exists() {
                println!("cargo:warning=使用ICO图标: {}", ico_path.display());
                res.set_icon(&ico_path.to_string_lossy());
            } else {
                let png_path = current_dir.join("assets").join("icon.png");
                if png_path.exists() {
                    println!("cargo:warning=使用PNG图标: {}", png_path.display());
                    res.set_icon(&png_path.to_string_lossy());
                }
            }
        }
        
        // 编译Windows资源
        if let Err(e) = res.compile() {
            eprintln!("编译Windows资源失败: {}", e);
        }
    }
} 