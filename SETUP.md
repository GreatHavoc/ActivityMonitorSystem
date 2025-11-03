# Activity Monitor Setup Guide

## Prerequisites

1. **Windows 10/11 (64-bit)**
2. **.NET 8.0 SDK or later** - [Download](https://dotnet.microsoft.com/download/dotnet/8.0)
3. **Ollama** - [Download](https://ollama.ai)
4. **GPU with 3GB+ VRAM** (recommended for optimal performance)
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
ollama pull qwen2.5-vl:3b

# Verify Ollama is running
curl http://localhost:11434/api/tags

# Test the model
ollama run qwen2.5-vl:3b "What can you see?"
```

**Note**: The 3B AWQ model requires ~2-3GB VRAM and provides excellent performance for real-time analysis.

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
    "OllamaModel": "qwen2.5-vl:3b",
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
ActivityMonitor.CLI.exe timeline --date 2024-01-15

# View detailed activity with AI analysis
ActivityMonitor.CLI.exe detailed --date 2024-01-15 --limit 50

# View activity summary
ActivityMonitor.CLI.exe summary --date 2024-01-15

# View activity statistics
ActivityMonitor.CLI.exe stats --date 2024-01-15

# Export activity report to JSON
ActivityMonitor.CLI.exe report --date 2024-01-15 --output report.json

# Query activities with custom filters
ActivityMonitor.CLI.exe query --start-date 2024-01-15 --end-date 2024-01-16 --process "chrome.exe"
```

### CLI Options

- `--date`: Specify date in YYYY-MM-DD format
- `--start-date`, `--end-date`: Date range for queries
- `--limit`: Maximum number of results to display
- `--process`: Filter by process name
- `--output`: Output file path for report export
- `--help`: Show help for each command

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
3. Verify date format (YYYY-MM-DD)
4. Use `--help` with any command for usage details

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
