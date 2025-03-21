# Blender翻译工具

这是一个使用Rust和egui框架创建的Blender翻译工具，可处理.mo、.po格式的文件。

## 功能

- 将.mo文件转换为.po文件格式
- 合并两个.po文件为一个
- 将.po文件转换为.mo文件
- 操作历史记录
- 设置菜单（含暗黑模式选项）
- 中文界面
- 默认深色主题

## 关于作者

本工具由凌川雪开发。

## 要求

- Rust 编译器和Cargo包管理器
- 如果你在Linux上运行，可能需要安装一些额外的依赖：
  ```
  sudo apt-get install libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libssl-dev
  ```

## 如何运行

1. 克隆此仓库:
   ```
   git clone <repository-url>
   cd rust-gui-example
   ```

2. 确保在`Fonts`目录中有微软雅黑字体文件 (`msyh.ttf`)，用于中文显示。

3. 使用Cargo编译并运行应用:
   ```
   cargo run
   ```

## 如何使用

### MO到PO转换

1. 在界面上选择"MO到PO"转换类型
2. 点击"浏览..."选择输入的MO文件
3. 点击"浏览..."选择输出PO文件的路径和名称
4. 点击"执行转换"按钮开始转换

### PO文件合并

1. 在界面上选择"PO合并"转换类型
2. 点击"浏览..."选择第一个PO文件
3. 点击"浏览..."选择第二个PO文件
4. 点击"浏览..."选择合并后的PO文件输出路径和名称
5. 点击"执行转换"按钮开始合并

### PO到MO转换

1. 在界面上选择"PO到MO"转换类型
2. 点击"浏览..."选择输入的PO文件
3. 点击"浏览..."选择输出MO文件的路径和名称
4. 点击"执行转换"按钮开始转换

## 文件格式说明

- `.mo`: Gettext翻译的二进制格式，供程序运行时使用
- `.po`: Gettext翻译格式，包含原文和翻译内容

## 关于项目

这个项目展示了如何使用Rust创建图形化应用程序，用于处理Blender翻译文件。它使用了egui框架和polib库。

### 字体支持

本应用使用微软雅黑字体以支持中文显示。字体文件需要放在项目根目录的`Fonts`文件夹下，命名为`msyh.ttf`。

### 应用图标

应用使用自定义的BLMM图标，存放于`assets`目录下。本程序使用两种方式设置图标：

1. 通过代码加载PNG图标（使用`include_bytes!`和`image`库）- 用于运行时窗口图标
2. 通过Windows资源文件设置系统图标（用于任务栏、文件浏览器和窗口标题栏）

#### 修改图标方法

如需修改图标：

1. 准备一个高质量的PNG图标（建议至少128x128像素）并保存为`assets/icon.png`
2. 准备一个标准的Windows ICO图标文件（包含多种分辨率）并保存为`assets/icon.ico`
   - ICO文件应包含至少以下分辨率：16x16, 32x32, 48x48, 256x256
   - 这对于在文件浏览器中显示图标非常重要
3. 重新编译项目以应用新图标：
   ```
   cargo clean
   cargo build --release
   ```

#### 图标问题排查

如果编译后EXE文件在文件浏览器中没有显示自定义图标，但运行时窗口有图标，可能是以下原因：

1. **ICO文件格式问题**：确保ICO文件是标准格式，包含多种分辨率
2. **资源设置问题**：检查`winres/app.rc`文件是否正确引用了ICO文件
3. **缓存问题**：Windows可能缓存了旧图标，尝试重启资源管理器或清除图标缓存

要解决这些问题，可以：

1. 使用专业工具（如IcoFX, Axialis IconWorkshop等）创建标准ICO文件
2. 确认资源编译过程中没有错误
3. 在PowerShell中运行以下命令清除Windows图标缓存：
   ```powershell
   Stop-Process -Name explorer -Force
   Start-Process explorer
   ```

#### 高级图标工具

可以使用以下工具生成标准ICO文件：

1. **ImageMagick**：
   ```
   cd 项目根目录
   powershell -ExecutionPolicy Bypass -File scripts/create_icon.ps1
   ```

2. **图像编辑软件**：使用Photoshop, GIMP等，配合ICO文件插件

#### 图标实现细节

本应用使用两种方式确保图标在不同场景下正确显示：

1. **运行时图标**：通过`main.rs`中的`load_icon`函数，将PNG图标加载为`eframe::IconData`
2. **系统图标**：通过`build.rs`中的`winres`库和自定义资源脚本，将ICO图标设置为Windows资源

这种双重设置方式确保图标在应用内部窗口、Windows任务栏和文件浏览器中都能正确显示。 