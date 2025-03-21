fn main() {
    // 告诉Cargo如果字体文件改变了，则重新编译
    println!("cargo:rerun-if-changed=Fonts/msyh.ttf");
    
    // 注意: 字体文件现在已经通过include_bytes!嵌入到可执行文件中
    // 不再需要复制字体文件到输出目录
} 