@echo off
echo ================================================
echo Activity Monitor (Rust) - Complete Cleanup
echo ================================================
echo.

echo Step 1: cargo clean...
cargo clean
echo.

echo Step 2: Deleting deploy folders...
if exist publish2 (
    echo Deleting publish2...
    rd /s /q publish2 2>nul
)
if exist publish_old (
    rd /s /q publish_old 2>nul
)
echo.

echo Step 3: Deleting build artifacts...
if exist target (
    echo Deleting target...
    rd /s /q target 2>nul
)
echo.

echo ================================================
echo Cleanup Complete!
echo ================================================
echo.
echo Ready for fresh build!
echo Next step: cargo build --release --workspace ^&^& powershell -File deploy.ps1
echo.
pause
