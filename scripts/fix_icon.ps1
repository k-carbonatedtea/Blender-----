# This script ensures the ICO icon is correctly embedded in the executable
# On Windows, EXE file icons need special handling

# Check if icon files exist
$icoPath = "assets\icon.ico"
$pngPath = "assets\icon.png"

if (-not (Test-Path $icoPath)) {
    Write-Host "ICO icon file not found: $icoPath"
    
    if (Test-Path $pngPath) {
        Write-Host "Trying to install rcedit tool..."
        
        # Try to install rcedit tool via cargo
        if (-not (Get-Command rcedit -ErrorAction SilentlyContinue)) {
            cargo install rcedit
        }
        
        # Check target executable
        $exePath = "target\release\rust-gui-example.exe"
        if (Test-Path $exePath) {
            Write-Host "Setting icon for executable $exePath..."
            rcedit $exePath --set-icon $pngPath
            Write-Host "Icon setting completed."
        } else {
            Write-Host "Executable not found: $exePath"
            Write-Host "Please run 'cargo build --release' to build the project first."
        }
    } else {
        Write-Host "PNG icon file not found: $pngPath"
        Write-Host "Please prepare an icon file before running this script."
    }
} else {
    Write-Host "ICO icon file found: $icoPath"
    
    # Check target executable
    $exePath = "target\release\rust-gui-example.exe"
    if (Test-Path $exePath) {
        Write-Host "Installing rcedit tool..."
        
        # Try to install rcedit tool via cargo
        if (-not (Get-Command rcedit -ErrorAction SilentlyContinue)) {
            cargo install rcedit
        }
        
        Write-Host "Setting icon for executable $exePath..."
        rcedit $exePath --set-icon $icoPath
        Write-Host "Icon setting completed."
    } else {
        Write-Host "Executable not found: $exePath"
        Write-Host "Please run 'cargo build --release' to build the project first."
    }
} 