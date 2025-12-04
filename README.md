# Activity Monitor - Windows Application

<p align="center">
  <img src="https://img.shields.io/badge/Windows-0078D4?style=for-the-badge&logo=windows&logoColor=white" alt="Windows"/>
  <img src="https://img.shields.io/badge/.NET-512BD4?style=for-the-badge&logo=dotnet&logoColor=white" alt=".NET"/>
  <img src="https://img.shields.io/badge/SQLite-003B57?style=for-the-badge&logo=sqlite&logoColor=white" alt="SQLite"/>
  <img src="https://img.shields.io/badge/Ollama-000000?style=for-the-badge&logo=ollama&logoColor=white" alt="Ollama"/>
  <img src="https://img.shields.io/badge/AI-FF6B35?style=for-the-badge&logo=openai&logoColor=white" alt="AI"/>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Activity_Monitor-🖥️-blue?style=flat-square" alt="Activity Monitor"/>
  <img src="https://img.shields.io/badge/Screen_Capture-📸-green?style=flat-square" alt="Screen Capture"/>
  <img src="https://img.shields.io/badge/CLI_Tools-💻-purple?style=flat-square" alt="CLI Tools"/>
  <img src="https://img.shields.io/badge/AI_Analysis-🤖-orange?style=flat-square" alt="AI Analysis"/>
  <img src="https://img.shields.io/badge/Privacy_Focused-🔒-red?style=flat-square" alt="Privacy Focused"/>
</p>

---

🖥️ **Activity Monitor** is a lightweight Windows application that monitors user activity through window focus tracking, idle detection, and intelligent screen capture with AI-powered activity analysis using Qwen2.5-VL.

## Features

- **Activity Tracking**: Monitors active window/app focus and idle periods using Win32 APIs
- **Smart Screen Capture**: Low-FPS screen capture using Windows Graphics Capture API
- **AI Analysis**: Qwen2.5-VL vision model for activity summaries and timeline labeling (using Ollama structured output for reliable JSON responses)
- **Queue System**: Handles overwhelming requests with priority-based processing
- **Event Storage**: SQLite-based timeline storage with efficient querying
- **CLI Tools**: Comprehensive command-line interface for viewing and exporting activity reports
- **Resource Efficient**: Minimal baseline overhead with on-demand capture triggers

## Architecture

### Core Components

1. **Activity Service** (`ActivityMonitorService`)
   - Runs as background process with auto-start via Windows Startup
   - Manages lifecycle and coordinates workers
   - Captures screen on focus changes and periodic intervals
   
2. **Activity Sensors** (`NativeSensors`)
   - Foreground window tracking via GetForegroundWindow
   - Idle detection using GetLastInputInfo
   - Process ID mapping for application identification

3. **Capture Worker** (`ScreenCaptureWorker`)
   - Windows.Graphics.Capture for secure screen recording
   - User-approved capture with system UI indicator
   - Low-FPS burst mode on triggers

4. **Inference Worker** (`OllamaInferenceClient`)
   - Qwen2.5-VL integration via Ollama server with structured output
   - Multimodal frame analysis with 3B AWQ model
   - Activity labeling and timeline generation with guaranteed JSON responses

5. **Queue System** (`RequestQueueManager`)
   - Priority-based request processing
   - Rate limiting and backpressure handling
   - Concurrent processing with configurable workers

6. **Storage** (`ActivityDatabase`)
   - SQLite for event timelines and inference results
   - Efficient indexing and querying
   - Optional frame retention

7. **CLI Interface** (`ActivityMonitor.CLI`)
   - Multiple viewing modes: timeline, detailed, summary, stats
   - JSON export functionality for comprehensive reports
   - Date range filtering and activity analysis

## Requirements

### System Requirements
- **Windows 10/11 (64-bit)**
- **6GB+ RAM** (for 3B model)
- **Admin privileges** for initial setup

### .NET Requirements

#### For Development/Building:
- **.NET 9.0 SDK** (latest SDK for building the project)

#### For Running Published Applications:
- **.NET 8.0 Runtime** (target runtime for published executables)

#### Download Links:
- [.NET 9.0 SDK](https://dotnet.microsoft.com/download/dotnet/9.0) - For development
- [.NET 8.0 Runtime](https://dotnet.microsoft.com/download/dotnet/8.0) - For running published apps

### AI/ML Requirements
- **Ollama** installed (with GPU support recommended)
  - Download: [https://ollama.ai](https://ollama.ai)
  - Model: `qwen2.5vl:3b` (3B vision-language model)

## Installation

### 1. Build the Project

```cmd
cd ActivityMonitor
dotnet build -c Release
```

### 2. Publish Applications

```cmd
# Publish both service and CLI to the same folder
dotnet publish ActivityMonitor.Service\ActivityMonitor.Service.csproj -c Release -o publish
dotnet publish ActivityMonitor.CLI\ActivityMonitor.CLI.csproj -c Release -o publish
```

> **Note:** After making code changes (such as the recent upgrade to Ollama structured output), you'll need to rebuild and republish your applications for the changes to take effect.

### 3. Setup Ollama with Qwen2.5-VL

```bash
# Install Ollama from https://ollama.ai

# Pull the Qwen2.5-VL model (3B AWQ quantized version)
ollama pull qwen2.5vl:3b

# Verify Ollama is running (default port 11434)
curl http://localhost:11434/api/tags
```

### 4. Configure Auto-Start

Create a shortcut in Windows Startup folder:
1. Press `Win + R`, type `shell:startup`
2. Create shortcut to `C:\ActivityMonitor\publish\ActivityMonitor.Service.exe`
3. Name it "Activity Monitor"

### 5. Test Installation

```cmd
cd C:\ActivityMonitor\publish

# Test service (run manually first)
ActivityMonitor.Service.exe

# In another terminal, test CLI
ActivityMonitor.CLI.exe stats
```

## Configuration

Edit `publish\appsettings.json`:

```json
{
  "ActivityMonitor": {
    "SamplingIntervalSeconds": 5,
    "IdleThresholdSeconds": 300,
    "CaptureSettings": {
      "FrameRate": 1,
      "MaxDurationSeconds": 30,
      "TriggerOnFocusChange": true,
      "TriggerOnIdleResume": true,
      "MaxFramesPerCapture": 1
    },
    "OllamaEndpoint": "http://localhost:11434",
    "OllamaModel": "qwen2.5vl:3b",
    "QueueSettings": {
      "MaxConcurrentTasks": 4,
      "MaxQueueSize": 100,
      "HighPriorityThreshold": 10,
      "ProcessingTimeoutSeconds": 0
    },
    "Storage": {
      "DatabasePath": "Data\\ActivityData.db",
      "RetainFrames": false,
      "CompactionIntervalHours": 24,
      "MaxEventAgeDays": 90
    }
  }
}
```

## Usage

### Starting the Service

The service starts automatically via the startup shortcut. To start manually:

```cmd
publish\ActivityMonitor.Service.exe
```

### CLI Commands

The CLI provides various commands to view and export activity data:

```cmd
# View activity timeline
publish\ActivityMonitor.CLI.exe timeline --from "2024-01-15" --to "2024-01-16"

# View detailed activity with AI analysis
publish\ActivityMonitor.CLI.exe detailed --from "2024-01-15" --to "2024-01-16" --limit 50

# View activity summary
publish\ActivityMonitor.CLI.exe summary --date "2024-01-15"

# View activity statistics
publish\ActivityMonitor.CLI.exe stats

# Export detailed activity report to JSON
publish\ActivityMonitor.CLI.exe report --from "2024-01-15" --to "2024-01-16" --output report.json

# Export LLM-optimized report (compact, narrative format)
publish\ActivityMonitor.CLI.exe report --from "2024-01-15" --to "2024-01-16" --llm

# View activity summary
publish\ActivityMonitor.CLI.exe summary --date "2024-01-15"

# View activity statistics
publish\ActivityMonitor.CLI.exe stats

# Export activity report to JSON
publish\ActivityMonitor.CLI.exe report --from "2024-01-15" --to "2024-01-16" --output report.json

# Query activities with custom filters
publish\ActivityMonitor.CLI.exe query --from "2024-01-15" --to "2024-01-16" --limit 100
```

### CLI Options

- `--from`: Start date in YYYY-MM-DD format (for timeline, detailed, report, query)
- `--to`: End date in YYYY-MM-DD format (for timeline, detailed, report, query)
- `--date`: Specific date in YYYY-MM-DD format (for summary)
- `--limit`: Maximum number of results to display (for detailed, query)
- `--output`: Output file path for report export (for report)
- `--llm`: Generate LLM-optimized report with narratives and patterns (for report)
- `--format`: Export format (json only, for report)
- `--help`: Show help for each command

## JSON Report Export

The `report` command supports two output formats:

### Standard Report (Schema v2.0)
Optimized JSON reports containing:

- **Check-in/Check-out Tracking**: First and last activity timestamps define actual working hours
- **Time Tracking**: Total tracked time based on actual activity window (not full 24-hour day)
- **Application Usage**: Per-application time spent with window details
- **AI Activity Analysis**: Content types, topics, and summaries for each activity
- **Timeline Data**: Optimized segments with UTC timestamps
- **Content Categories**: Activity breakdown by type (coding, browsing, documents, etc.)

**Report Schema v2.0** - Optimized for 50-70% smaller file size:
```json
{
  "SchemaVersion": "2.0",
  "GeneratedAtUtc": "2024-01-15T20:30:00Z",
  "RangeStartUtc": "2024-01-15T00:00:00Z",
  "RangeEndUtc": "2024-01-15T23:59:59Z",
  "CheckInTimeUtc": "2024-01-15T09:00:00Z",
  "CheckOutTimeUtc": "2024-01-15T17:30:00Z",
  "TotalTrackedSeconds": 30600,
  "TotalActiveSeconds": 28000,
  "TotalIdleSeconds": 2600,
  "Applications": [
    {
      "ProcessName": "Visual Studio Code",
      "TotalActiveSeconds": 14520,
      "Windows": [{"Title": "Program.cs", "ActiveSeconds": 8000}],
      "InsightIndices": [0, 5, 12, 23]
    }
  ],
  "DetailedActivities": [...],
  "ContentTypeBreakdown": [...],
  "Segments": [
    {
      "EndUtc": "2024-01-15T09:05:00Z",
      "DurationSeconds": 300,
      "IsIdle": false,
      "ProcessName": "Code"
    }
  ]
}
```

**Key features of v2.0:**
- `InsightIndices` references `DetailedActivities` array (deduplication)
- UTC-only timestamps (clients convert to local)
- No formatted strings (clients compute from seconds)
- Optimized segments (only `EndUtc`, start inferred from previous)
- Null values omitted, `VisibleText` truncated to 200 chars

### LLM-Optimized Report (Schema v3.0-llm)

Compact, narrative-focused reports optimized for LLM consumption:

```cmd
publish\ActivityMonitor.CLI.exe report --from "2024-01-15" --llm
```

The `--llm` flag generates reports with:
- **Executive Summary**: One-liner, narrative description, accomplishments, technologies
- **Hourly Breakdown**: Hour-by-hour view of focus areas, applications, topics
- **Projects**: Inferred project groupings with time spent and work summaries
- **Top Activities**: 20 most informative activities with full context
- **Significant Moments**: Key events, focus sessions, and transitions
- **Patterns**: Productivity insights (most productive hour, context switches, observations)

**LLM Report Structure:**
```json
{
  "SchemaVersion": "3.0-llm",
  "ReportDate": "2024-01-15",
  "WorkingHours": {"Start": "09:23", "End": "17:45", "TotalFormatted": "8h 22m"},
  "ExecutiveSummary": {
    "OneLiner": "Focused on fashion_agent (2h 30m) with 137 activities across 12 apps",
    "Narrative": "On January 15, work started at 9:23 AM and ended at 5:45 PM...",
    "Accomplishments": "Edited code in 15 sessions. Researched: LangGraph, PostgreSQL...",
    "Technologies": ["Python", "LangGraph", "Docker"]
  },
  "HourlyBreakdown": [{"Hour": "14:00-14:59", "PrimaryFocus": "code", "Topics": [...]}],
  "Projects": [{"Name": "fashion_agent", "TimeFormatted": "2h 30m", "KeyFiles": [...]}],
  "TopActivities": [...],
  "SignificantMoments": [...],
  "Patterns": {
    "MostProductiveHour": "14:00 (23 activities)",
    "LongestFocusSession": "VS Code (45 minutes)",
    "Observations": ["Heavy coding day (45% code activities)"]
  }
}
```

Use LLM reports for AI-powered productivity analysis, daily standups, or integration with LLM-based tools and agents.

Use these reports for productivity analysis, time tracking, or integration with other tools.

## Privacy & Security

- **User Consent**: System picker UI for capture approval
- **Visual Indicator**: Yellow border shows active capture
- **Minimal Data**: Only captures on triggers, not continuously
- **Local Storage**: All data stored locally in SQLite
- **Configurable Retention**: Control frame storage duration

## Performance

- **Baseline CPU**: < 1% during idle monitoring
- **Memory**: ~50MB without active capture
- **GPU Usage**: Spikes only during inference
- **Storage**: ~10MB per hour of activity (without frames)

## Troubleshooting

### Service Won't Start
- Check Windows Event Viewer for errors (Windows Logs → Application)
- Verify .NET 8.0 runtime is installed
- Ensure Ollama is running and accessible
- Check that the startup shortcut points to the correct path

### Capture Not Working
- Verify Graphics Capture capability in app manifest
- Check Windows Graphics Capture is enabled in Settings
- Run the service as Administrator
- Check capture logs in the publish folder

### High CPU/Memory Usage
- Reduce capture frame rate in `publish\appsettings.json`
- Increase sampling interval
- Lower concurrent queue workers
- Check Ollama resource usage with `ollama ps`

### CLI Commands Not Working
- Ensure you're running from the publish folder
- Check that ActivityData.db exists in publish\Data\
- Verify date format (YYYY-MM-DD) and use quotes around dates
- Use `--help` with any command for usage details
- Timeline, detailed, report, and query commands require `--from` and/or `--to` dates
- Summary command uses `--date` for a single day

### Database Issues
- Check publish\Data\ActivityData.db file exists
- Ensure write permissions to the publish folder
- Database corruption: delete ActivityData.db and restart service

## Development

### Build Requirements
- Visual Studio 2022 or VS Code
- .NET 8.0 SDK
- Windows 10 SDK

### Build and Publish

```cmd
# Build all projects
dotnet build -c Release

# Publish both service and CLI to shared folder
dotnet publish ActivityMonitor.Service\ActivityMonitor.Service.csproj -c Release -o publish
dotnet publish ActivityMonitor.CLI\ActivityMonitor.CLI.csproj -c Release -o publish
```

### Run in Debug Mode

```cmd
# Run service directly
dotnet run --project ActivityMonitor.Service

# In another terminal, test CLI
dotnet run --project ActivityMonitor.CLI -- stats
```

### Running Tests

```cmd
dotnet test
```

## Project Structure

```
ActivityMonitor/
├── ActivityMonitor.sln                    # Solution file
├── ActivityMonitor.Service/               # Windows service host
│   ├── ActivityMonitor.Service.csproj
│   ├── Program.cs
│   ├── ActivityMonitorService.cs
│   ├── appsettings.json
│   └── appsettings.Development.json
├── ActivityMonitor.CLI/                   # Command-line interface
│   ├── ActivityMonitor.CLI.csproj
│   └── Program.cs
├── ActivityMonitor.Core/                  # Core business logic
│   ├── ActivityMonitor.Core.csproj
│   ├── Sensors/                           # Activity sensors
│   │   ├── NativeSensors.cs
│   │   ├── FocusTracker.cs
│   │   └── IdleDetector.cs
│   ├── Capture/                           # Screen capture
│   │   ├── ScreenCaptureWorker.cs
│   │   └── CaptureManager.cs
│   ├── Inference/                         # AI inference
│   │   ├── VisionInferenceClient.cs
│   │   └── QwenVLPrompts.cs
│   ├── Queue/                             # Request queue
│   │   ├── RequestQueueManager.cs
│   │   ├── PriorityQueue.cs
│   │   └── QueueMetrics.cs
│   └── Storage/                           # Data storage
│       ├── ActivityDatabase.cs
│       └── Models/
├── ActivityMonitor.Common/                # Shared models & utilities
│   ├── ActivityMonitor.Common.csproj
│   ├── Models/
│   └── Configuration/
├── publish/                               # Published applications
│   ├── ActivityMonitor.Service.exe
│   ├── ActivityMonitor.CLI.exe
│   ├── Data/
│   │   └── ActivityData.db
│   └── appsettings.json
├── .gitignore
├── README.md
├── SETUP.md
├── HOW_TO_VIEW_ACTIVITY_LOGS.md
├── FRESH_START_GUIDE.md
├── LICENSE
└── cleanup.bat
```

## License

MIT License - See LICENSE file for details

## Contributing

1. Fork the repository
2. Create a feature branch
3. Commit your changes
4. Push to the branch
5. Create a Pull Request

## Acknowledgments

- Ollama team for easy local LLM deployment
- Qwen team for vision-language models
- Microsoft for Windows Graphics Capture API
- Community for AWQ quantization methods
