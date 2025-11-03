# How to View All User Activity Logs

## Quick Start Commands

### 1. **Complete Timeline View** (Recommended for seeing everything)
```cmd
cd C:\ActivityMonitor\publish
ActivityMonitor.CLI.exe timeline --date 2024-01-15
```
This shows a chronological timeline of ALL activities with:
- Timestamps
- Application names
- Topics worked on
- Actions performed
- Summary of what was on screen
- Visible text/URLs

### 2. **Detailed Activity Analysis** (Best for comprehensive details)
```cmd
ActivityMonitor.CLI.exe detailed --date 2024-01-15 --limit 100
```
Shows rich, detailed cards for each activity including:
- Full AI analysis
- Application name
- Content type (code, article, video, etc.)
- Specific topic
- What action user was doing
- Complete summary
- Extracted text from screen
- Confidence level

### 3. **Daily Summary** (Best for overview)
```cmd
ActivityMonitor.CLI.exe summary --date 2024-01-15
```
Shows aggregated daily summary:
- Activity breakdown by category
- Top applications used
- Time distribution chart (coding, browsing, documents, etc.)
- Topics covered

### 4. **Activity Statistics**
```cmd
ActivityMonitor.CLI.exe stats --date 2024-01-15
```
Shows detailed statistics and metrics

### 5. **Export Activity Report** (New!)
```cmd
ActivityMonitor.CLI.exe report --date 2024-01-15 --output activity_report.json
```
Exports comprehensive activity data to JSON format for external analysis

## Examples

### See Everything User Did Today
```cmd
cd C:\ActivityMonitor\publish
ActivityMonitor.CLI.exe timeline --date 2024-01-15
```

**Output Example:**
```
──────────── Activity Timeline: 2024-01-15 to 2024-01-15 ────────────

14:30:15 ● Web Browsing - AI Technology News
         App: Google Chrome
         Topic: Qwen2.5-VL Vision Language Model
         Action: reading
         Summary: User reading about Qwen2.5-VL model on Hugging Face
         Text: Qwen/Qwen2.5-VL-3B-Instruct-AWQ | huggingface.co

14:35:22 ● Coding - C# Backend Development
         App: Visual Studio Code
         Topic: Activity Monitor Service Implementation
         Action: writing code
         Summary: Implementing periodic screen capture in ActivityMonitorService
         Text: class ActivityMonitorService : BackgroundService { ... }

14:40:10 ● Document - Technical Writing
         App: Microsoft Word
         Topic: Project Documentation
         Action: editing
         Summary: Writing setup instructions for Activity Monitor
```

### See Detailed Analysis for Last 50 Activities
```cmd
ActivityMonitor.CLI.exe detailed --date 2024-01-15 --limit 50
```

**Output Example:**
```
╭────────────────── 2024-01-15 14:30:15 ──────────────────╮
│ Web Browsing - AI Technology News                       │
│                                                          │
│ Application: Google Chrome                              │
│ Content Type: article                                   │
│ Topic: Qwen2.5-VL Vision Language Model                │
│ Action: reading                                         │
│                                                          │
│ Summary:                                                │
│ User is reading a technical article about the Qwen2.5-VL│
│ vision-language model on Hugging Face. The page shows   │
│ model architecture details, performance benchmarks, and │
│ usage examples for multimodal tasks.                    │
│                                                          │
│ Visible Text:                                           │
│ Qwen/Qwen2.5-VL-3B-Instruct-AWQ - A 3B parameter vision│
│ language model | huggingface.co/Qwen                    │
│                                                          │
│ Confidence: 95%                                         │
╰──────────────────────────────────────────────────────────╯
```

### See Daily Summary with Charts
```cmd
ActivityMonitor.CLI.exe summary --date 2024-01-15
```

**Output Example:**
```
───────────────────── Daily Summary: 2024-01-15 ─────────────────────

Activity by Content Type
┌──────────────┬───────┬─────────────────────────────────┐
│ Category     │ Count │ Topics Covered                  │
├──────────────┼───────┼─────────────────────────────────┤
│ code         │ 45    │ C# Development, Python Scripts  │
│ article      │ 32    │ AI/ML News, Tech Documentation  │
│ document     │ 12    │ Project Reports, Meeting Notes  │
│ video        │ 8     │ Tech Tutorials, Conference Talk │
│ email        │ 15    │ Work Correspondence             │
└──────────────┴───────┴─────────────────────────────────┘

Top Applications
┌────────────────────┬────────────────┐
│ Application        │ Activity Count │
├────────────────────┼────────────────┤
│ Visual Studio Code │ 45             │
│ Google Chrome      │ 40             │
│ Microsoft Teams    │ 15             │
│ Microsoft Word     │ 12             │
└────────────────────┴────────────────┘

Activity Distribution
Coding          ████████████████████ 45
Web Browsing    ████████████████ 40
Documents       ███████ 12
Videos          ████ 8
Communication   ████████ 15
```

## Date Range Queries

### Last 7 Days
```cmd
ActivityMonitor.CLI.exe timeline --start-date 2024-01-08 --end-date 2024-01-15
```

### Specific Date Range
```cmd
ActivityMonitor.CLI.exe detailed --start-date 2024-01-10 --end-date 2024-01-10 --limit 200
```

### Yesterday's Summary
```cmd
ActivityMonitor.CLI.exe summary --date 2024-01-14
```

## Export to File

### Save Timeline to Text File
```cmd
ActivityMonitor.CLI.exe timeline --date 2024-01-15 > my_activity_log.txt
```

### Save Detailed Analysis
```cmd
ActivityMonitor.CLI.exe detailed --date 2024-01-15 --limit 500 > detailed_activity.txt
```

### Export JSON Report (New!)
```cmd
ActivityMonitor.CLI.exe report --date 2024-01-15 --output activity_report.json
```

The JSON export includes all activity data in structured format for external analysis tools.

## Filtering Tips

### See Only Recent Activities (Last 2 Hours)
```cmd
ActivityMonitor.CLI.exe detailed --start-date 2024-01-15 --end-date 2024-01-15 --limit 50
```

### Full Day Overview
```cmd
ActivityMonitor.CLI.exe timeline --start-date 2024-01-15 --end-date 2024-01-15
```

### Filter by Application
```cmd
ActivityMonitor.CLI.exe query --start-date 2024-01-15 --end-date 2024-01-15 --process "chrome.exe"
```

## Batch Commands for Weekly Report

Create a file `weekly_report.bat`:
```batch
@echo off
echo ========================================
echo WEEKLY ACTIVITY REPORT
echo ========================================
echo.

echo Monday Summary:
ActivityMonitor.CLI.exe summary --date 2024-01-08
echo.

echo Tuesday Summary:
ActivityMonitor.CLI.exe summary --date 2024-01-09
echo.

echo Wednesday Summary:
ActivityMonitor.CLI.exe summary --date 2024-01-10
echo.

echo Thursday Summary:
ActivityMonitor.CLI.exe summary --date 2024-01-11
echo.

echo Friday Summary:
ActivityMonitor.CLI.exe summary --date 2024-01-12
echo.

pause
```

## Direct Database Queries

For advanced users, you can query the database directly:

```cmd
cd C:\ActivityMonitor\publish
sqlite3 Data\ActivityData.db "SELECT * FROM InferenceResults ORDER BY ProcessedAt DESC LIMIT 10;"
```

## Rebuild CLI with New Commands

```cmd
cd C:\ActivityMonitor
dotnet clean
dotnet publish ActivityMonitor.CLI\ActivityMonitor.CLI.csproj -c Release -o publish
```

## All Available Commands

```cmd
# Show help
ActivityMonitor.CLI.exe --help

# Timeline view (chronological with details)
ActivityMonitor.CLI.exe timeline --date 2024-01-15

# Detailed analysis (rich cards with full info)
ActivityMonitor.CLI.exe detailed --date 2024-01-15 --limit 100

# Daily summary (aggregated stats and charts)
ActivityMonitor.CLI.exe summary --date 2024-01-15

# Activity statistics
ActivityMonitor.CLI.exe stats --date 2024-01-15

# Export JSON report (new!)
ActivityMonitor.CLI.exe report --date 2024-01-15 --output report.json

# Raw event query with filters
ActivityMonitor.CLI.exe query --start-date 2024-01-15 --end-date 2024-01-16 --process "chrome.exe"
```

---

## 🎯 Recommended Usage

**For comprehensive daily review:**
1. Start with `summary` to see high-level overview
2. Use `timeline` to see chronological flow of activities
3. Dive into `detailed` for specific periods of interest
4. Export `report` for data analysis in other tools

**Example workflow:**
```cmd
cd C:\ActivityMonitor\publish

# 1. Get daily overview
ActivityMonitor.CLI.exe summary --date 2024-01-15

# 2. See timeline of what you did
ActivityMonitor.CLI.exe timeline --date 2024-01-15

# 3. Deep dive into afternoon work
ActivityMonitor.CLI.exe detailed --start-date 2024-01-15 --end-date 2024-01-15 --limit 50

# 4. Export for analysis
ActivityMonitor.CLI.exe report --date 2024-01-15 --output daily_report.json
```

This will give you **complete visibility** into everything the user did! 🎉
