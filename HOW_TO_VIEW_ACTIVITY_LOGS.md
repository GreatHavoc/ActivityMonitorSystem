# How to View All User Activity Logs

## Quick Start Commands

### 1. **Complete Timeline View** (Recommended for seeing everything)
```cmd
cd C:\ActivityMonitor\publish
ActivityMonitor.CLI.exe timeline --from "2024-01-15" --to "2024-01-16"
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
ActivityMonitor.CLI.exe detailed --from "2024-01-15" --to "2024-01-16" --limit 100
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
ActivityMonitor.CLI.exe summary --date "2024-01-15"
```
Shows aggregated daily summary:
- Activity breakdown by category
- Top applications used
- Time distribution chart (coding, browsing, documents, etc.)
- Topics covered

### 4. **Activity Statistics**
```cmd
ActivityMonitor.CLI.exe stats
```
Shows detailed statistics and metrics

### 5. **Export Activity Report**
```cmd
ActivityMonitor.CLI.exe report --from "2024-01-15" --to "2024-01-16" --output activity_report.json
```
Exports comprehensive activity data to JSON format for external analysis

### 6. **Export LLM-Optimized Report** (New!)
```cmd
ActivityMonitor.CLI.exe report --from "2024-01-15" --to "2024-01-16" --llm
```
Exports a compact, narrative-focused report optimized for LLM consumption and analysis

## Examples

### See Everything User Did Today
```cmd
cd C:\ActivityMonitor\publish
ActivityMonitor.CLI.exe timeline --from "2024-01-15" --to "2024-01-16"
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
ActivityMonitor.CLI.exe detailed --from "2024-01-15" --to "2024-01-16" --limit 50
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
ActivityMonitor.CLI.exe summary --date "2024-01-15"
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
ActivityMonitor.CLI.exe timeline --from "2024-01-08" --to "2024-01-15"
```

### Specific Date Range
```cmd
ActivityMonitor.CLI.exe detailed --from "2024-01-10" --to "2024-01-10" --limit 200
```

### Yesterday's Summary
```cmd
ActivityMonitor.CLI.exe summary --date "2024-01-14"
```

## Export to File

### Save Timeline to Text File
```cmd
ActivityMonitor.CLI.exe timeline --from "2024-01-15" --to "2024-01-16" > my_activity_log.txt
```

### Save Detailed Analysis
```cmd
ActivityMonitor.CLI.exe detailed --from "2024-01-15" --to "2024-01-16" --limit 500 > detailed_activity.txt
```

### Export JSON Report
```cmd
ActivityMonitor.CLI.exe report --from "2024-01-15" --to "2024-01-16" --output report.json
```

The JSON export creates a comprehensive activity report containing:

- **Time Summary**: Total tracked, active, and idle time in seconds
- **Check-In/Check-Out**: First and last activity timestamps of the day
- **Application Usage**: Detailed breakdown by application with time spent per window
- **Activity Insights**: Deduplicated AI-analyzed activities with content types, topics, and summaries
- **Content Categories**: Breakdown by activity type (coding, browsing, documents, etc.)
- **Timeline Segments**: Chronological timeline with UTC timestamps

**Report Structure (Schema v2.0):**
```json
{
  "SchemaVersion": "2.0",
  "GeneratedAtUtc": "2024-01-15T20:30:00Z",
  "RangeStartUtc": "2024-01-15T00:00:00Z",
  "RangeEndUtc": "2024-01-15T23:59:59Z",
  "CheckInTimeUtc": "2024-01-15T09:00:00Z",
  "CheckOutTimeUtc": "2024-01-15T17:30:00Z",
  "TotalTrackedSeconds": 30615,
  "TotalActiveSeconds": 24322,
  "TotalIdleSeconds": 6293,
  "AllInsights": [...],
  "Applications": [...],
  "DetailedActivities": [...],
  "ContentTypeBreakdown": [...],
  "Segments": [...]
}
```

Use this for data analysis, reporting, or integration with other tools.

### Export LLM-Optimized Report (New!)
```cmd
ActivityMonitor.CLI.exe report --from "2024-01-15" --to "2024-01-16" --llm
ActivityMonitor.CLI.exe report --from "2024-01-15" --llm --output weekly-summary.json
```

The `--llm` flag generates a compact, narrative-focused report (Schema v3.0-llm) optimized for LLM consumption:

- **Executive Summary**: One-liner, narrative description, accomplishments, technologies used
- **Hourly Breakdown**: Hour-by-hour view of focus areas, applications, and topics
- **Projects**: Inferred project groupings with time spent, files edited, and work summaries
- **Top Activities**: 20 most informative activities with full context and summaries
- **Significant Moments**: Key events like focus sessions, context switches, and transitions
- **Patterns**: Productivity insights like most productive hour, longest focus session, observations

**LLM Report Structure (Schema v3.0-llm):**
```json
{
  "SchemaVersion": "3.0-llm",
  "ReportDate": "2024-01-15",
  "WorkingHours": {
    "Start": "09:23",
    "End": "17:45",
    "TotalMinutes": 502,
    "TotalFormatted": "8h 22m"
  },
  "ExecutiveSummary": {
    "OneLiner": "Focused on fashion_agent (2h 30m) with 137 activities across 12 apps",
    "Narrative": "On January 15, 2024, work started at 9:23 AM and ended at 5:45 PM...",
    "Accomplishments": "Edited code in 15 sessions. Researched: LangGraph, PostgreSQL...",
    "Technologies": ["Python", "LangGraph", "PostgreSQL", "Docker"]
  },
  "HourlyBreakdown": [
    {"Hour": "09:00-09:59", "PrimaryFocus": "code", "Topics": ["graph.py", "state management"]}
  ],
  "Projects": [
    {
      "Name": "fashion_agent",
      "Type": "Software Development",
      "TimeFormatted": "2h 30m",
      "KeyFiles": ["graph.py", "data_collector.py"],
      "TopicsWorkedOn": ["LangGraph", "PostgreSQL checkpointer"]
    }
  ],
  "TopActivities": [...],
  "SignificantMoments": [...],
  "Patterns": {
    "MostProductiveHour": "14:00 (23 activities)",
    "LongestFocusSession": "VS Code (45 minutes)",
    "ContextSwitches": 28,
    "Observations": ["Heavy coding day (45% code activities)"]
  }
}
```

Use this format for:
- AI-powered productivity analysis
- Daily standup summaries
- Time tracking reports
- Integration with LLM-based tools and agents

## Filtering Tips

### See Only Recent Activities (Last 2 Hours)
```cmd
ActivityMonitor.CLI.exe detailed --from "2024-01-15" --to "2024-01-15" --limit 50
```

### Full Day Overview
```cmd
ActivityMonitor.CLI.exe timeline --from "2024-01-15" --to "2024-01-15"
```

### Filter by Application
```cmd
ActivityMonitor.CLI.exe query --from "2024-01-15" --to "2024-01-15" --limit 100
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
ActivityMonitor.CLI.exe timeline --from "2024-01-15" --to "2024-01-16"

# Detailed analysis (rich cards with full info)
ActivityMonitor.CLI.exe detailed --from "2024-01-15" --to "2024-01-16" --limit 100

# Daily summary (aggregated stats and charts)
ActivityMonitor.CLI.exe summary --date "2024-01-15"

# Activity statistics
ActivityMonitor.CLI.exe stats

# Export JSON report (detailed format)
ActivityMonitor.CLI.exe report --from "2024-01-15" --to "2024-01-16" --output report.json

# Export LLM-optimized report (compact, narrative format)
ActivityMonitor.CLI.exe report --from "2024-01-15" --to "2024-01-16" --llm

# Raw event query with filters
ActivityMonitor.CLI.exe query --from "2024-01-15" --to "2024-01-16" --limit 100
```

---

## 🎯 Recommended Usage

**For comprehensive daily review:**
1. Start with `summary` to see high-level overview
2. Use `timeline` to see chronological flow of activities
3. Dive into `detailed` for specific periods of interest
4. Export `report` for data analysis in other tools
5. Use `report --llm` for AI-powered analysis or daily summaries

**Example workflow:**
```cmd
cd C:\ActivityMonitor\publish

# 1. Get daily overview
ActivityMonitor.CLI.exe summary --date 2024-01-15

# 2. See timeline of what you did
ActivityMonitor.CLI.exe timeline --from "2024-01-15" --to "2024-01-16"

# 3. Deep dive into afternoon work
ActivityMonitor.CLI.exe detailed --from "2024-01-15" --to "2024-01-15" --limit 50

# 4. Export detailed report for analysis
ActivityMonitor.CLI.exe report --from "2024-01-15" --to "2024-01-16" --output daily_report.json

# 5. Export LLM-friendly summary for AI analysis
ActivityMonitor.CLI.exe report --from "2024-01-15" --llm
```

This will give you **complete visibility** into everything the user did! 🎉
