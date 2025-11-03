@echo off
echo ================================================
echo Activity Monitor - Complete Cleanup
echo ================================================
echo.

cd /d C:\ActivityMonitor

echo Step 1: Running dotnet clean...
dotnet clean
echo.

echo Step 2: Waiting for file handles to release...
timeout /t 3 /nobreak
echo.

echo Step 3: Deleting publish folders...
if exist publish (
    echo Deleting publish...
    rd /s /q publish 2>nul
    if exist publish (
        echo WARNING: Could not delete publish folder - may be locked
        echo Renaming to publish_old instead...
        move publish publish_old_%RANDOM%
    ) else (
        echo SUCCESS: publish deleted
    )
)

if exist publish_old (
    echo Deleting publish_old...
    rd /s /q publish_old 2>nul
)
echo.

echo Step 4: Deleting bin folders...
for /d %%i in (ActivityMonitor.*) do (
    if exist "%%i\bin" (
        echo Deleting %%i\bin
        rd /s /q "%%i\bin" 2>nul
    )
)
echo.

echo Step 5: Deleting obj folders...
for /d %%i in (ActivityMonitor.*) do (
    if exist "%%i\obj" (
        echo Deleting %%i\obj
        rd /s /q "%%i\obj" 2>nul
    )
)
echo.

echo Step 6: Deleting temporary documentation...
if exist DATABASE_CODE_VERIFICATION.md del DATABASE_CODE_VERIFICATION.md
if exist MIGRATION_OLLAMA.md del MIGRATION_OLLAMA.md
if exist project_draft.txt del project_draft.txt
echo.

echo ================================================
echo Cleanup Complete!
echo ================================================
echo.

echo Checking what remains...
dir /b /a:d ActivityMonitor.* 2>nul
echo.

echo Ready for fresh build!
echo Next step: dotnet restore
echo.
pause
