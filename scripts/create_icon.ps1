# 检查是否安装了ImageMagick
$magickPath = (Get-Command magick -ErrorAction SilentlyContinue).Source

if ($null -eq $magickPath) {
    Write-Host "未找到ImageMagick。请安装ImageMagick以便生成图标。"
    Write-Host "您可以从 https://imagemagick.org/script/download.php 下载ImageMagick。"
    Write-Host "或者使用其他图标编辑工具手动创建ICO文件。"
    exit 1
}

# 检查assets目录
if (-not (Test-Path "assets")) {
    New-Item -ItemType Directory -Force -Path "assets"
}

# 确保SVG文件存在
if (-not (Test-Path "assets/icon.svg")) {
    Write-Host "未找到assets/icon.svg文件。请先创建SVG图标。"
    exit 1
}

# 使用ImageMagick转换SVG到PNG（多种尺寸）
$sizes = @(16, 32, 48, 64, 128, 256)

foreach ($size in $sizes) {
    Write-Host "生成 ${size}x${size} 像素的PNG图标..."
    magick convert -background none -size ${size}x${size} assets/icon.svg assets/icon-${size}.png
}

# 合并所有PNG到一个ICO文件
Write-Host "创建ICO文件..."
magick convert assets/icon-*.png assets/icon.ico

Write-Host "完成！图标已保存到 assets/icon.ico" 