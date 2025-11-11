# Activity Monitor Setup Guide

## Prerequisites

1. **Windows 10/11 (64-bit)**
2. **.NET 8.0 SDK or later** - [Download](https://dotnet.microsoft.com/download/dotnet/8.0)
3. **Ollama** - [Download](https://ollama.ai)
4. **GPU with 2GB+ VRAM** (recommended for optimal performance)
5. **Administrator privileges** - Required for initial setup

## Quick Start

### 1. Build the Project

```cmd
cd ActivityMonitor
dotnet restore
dotnet build -c Release
```

### 2. Publish Applications

```cmd
# Publish both service and CLI to the same folder
dotnet publish ActivityMonitor.Service\ActivityMonitor.Service.csproj -c Release -o publish
dotnet publish ActivityMonitor.CLI\ActivityMonitor.CLI.csproj -c Release -o publish
```

### 3. Setup Ollama with Qwen2.5-VL

Download and install Ollama from [https://ollama.ai](https://ollama.ai)

Then pull the Qwen2.5-VL model:

```bash
# Pull the 3B AWQ quantized model (smaller, faster)
ollama pull qwen2.5vl:3b

# Verify Ollama is running
curl http://localhost:11434/api/tags

# Test the model
ollama run qwen2.5vl:3b "What can you see?"
```

**Note**: The 2B model requires ~1-2GB VRAM and provides excellent performance for real-time analysis.

### 4. Configure Auto-Start

Create a shortcut in Windows Startup folder for automatic startup:

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

## Using the CLI Tool

Navigate to the publish folder and use these commands:

```cmd
cd C:\ActivityMonitor\publish

# View activity timeline
ActivityMonitor.CLI.exe timeline --from "2024-01-15" --to "2024-01-16"

# View detailed activity with AI analysis
ActivityMonitor.CLI.exe detailed --from "2024-01-15" --to "2024-01-16" --limit 50

# View activity summary
ActivityMonitor.CLI.exe summary --date "2024-01-15"

# View activity statistics
ActivityMonitor.CLI.exe stats

# Export activity report to JSON
ActivityMonitor.CLI.exe report --from "2024-01-15" --to "2024-01-16" --output report.json

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
- `--format`: Export format (json only, for report)
- `--help`: Show help for each command

## JSON Report Export

The `report` command generates detailed JSON reports for comprehensive activity analysis:

- **Time summaries** with active/idle breakdowns
- **Per-application usage** with window-level details
- **AI-powered activity insights** including content types and topics
- **Timeline segments** with precise timestamps
- **Content type categorization** for productivity analysis

Reports are saved to the specified output path and can be used for external analysis tools or custom reporting.

## Troubleshooting

### Service Won't Start

1. Check Windows Event Viewer for errors (Windows Logs → Application)
2. Verify .NET 8.0 runtime is installed
3. Ensure Ollama is running: `ollama list`
4. Check that the startup shortcut points to the correct path
5. Verify publish folder permissions

### High CPU/Memory Usage

1. Reduce `MaxConcurrentTasks` in `publish\appsettings.json`
2. Increase `SamplingIntervalSeconds`
3. Lower capture frame rate
4. Check Ollama resource usage: `ollama ps`
5. Ensure model is using GPU: check Ollama logs

### CLI Commands Not Working

1. Ensure you're running from the publish folder
2. Check that ActivityData.db exists in publish\Data\
3. Verify date format (YYYY-MM-DD) and use quotes around dates
4. Use `--help` with any command for usage details
5. Timeline, detailed, report, and query commands require `--from` and/or `--to` dates
6. Summary command uses `--date` for a single day

### Database Issues

1. Check publish\Data\ActivityData.db file exists
2. Ensure write permissions to the publish folder
3. Database corruption: delete ActivityData.db and restart service

### Ollama Connection Errors

1. Verify Ollama is running: `ollama list`
2. Check if model is loaded: `ollama ps`
3. Test endpoint: `curl http://localhost:11434/api/tags`
4. Ensure firewall allows connections to port 11434
5. Check Ollama logs for errors

## Uninstalling

```cmd
# Stop any running service processes
taskkill /f /im ActivityMonitor.Service.exe

# Remove from Windows Startup
# Press Win + R, type shell:startup
# Delete the "Activity Monitor" shortcut

# Remove files
rmdir /s "C:\ActivityMonitor"
```

## Performance Tuning

### For Low-End Systems (Integrated GPU or Limited VRAM)
- Use CPU mode: Set `OLLAMA_NUM_GPU=0` environment variable
- Increase sampling interval to 10+ seconds
- Set MaxConcurrentTasks to 1-2
- Reduce capture frame rate to 0.5 FPS
- Disable frame retention
- Consider using even smaller models if available

### For High-End Systems (Dedicated GPU with 6GB+ VRAM)
- Decrease sampling interval to 2-3 seconds
- Increase MaxConcurrentTasks to 6-8
- Use higher frame rates (2-3 FPS)
- Enable frame retention for detailed analysis
- Consider using larger models: `ollama pull qwen2.5-vl:7b`

## Security Considerations

- All data is stored locally
- Screen captures require user consent via system UI
- No data is sent to external servers (except local vLLM)
- Consider encrypting the database for sensitive environments
- Regularly review and clean old data

## Getting Help

- Check the logs in Event Viewer
- Review log files in `Logs/` directory
- Open an issue on GitHub
- Consult the README for detailed documentation
