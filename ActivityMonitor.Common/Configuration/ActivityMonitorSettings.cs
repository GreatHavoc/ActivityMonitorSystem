namespace ActivityMonitor.Common.Configuration;

public class ActivityMonitorSettings
{
    public int SamplingIntervalSeconds { get; set; } = 5;
    public int IdleThresholdSeconds { get; set; } = 300;
    public CaptureSettings CaptureSettings { get; set; } = new();
    public string OllamaEndpoint { get; set; } = "http://localhost:11434";
    public string OllamaModel { get; set; } = "qwen2.5-vl:3b";
    public QueueSettings QueueSettings { get; set; } = new();
    public StorageSettings Storage { get; set; } = new();
}

public class CaptureSettings
{
    public int FrameRate { get; set; } = 1;
    public int MaxDurationSeconds { get; set; } = 30;
    public bool TriggerOnFocusChange { get; set; } = true;
    public bool TriggerOnIdleResume { get; set; } = true;
    public int MaxFramesPerCapture { get; set; } = 30;
}

public class QueueSettings
{
    public int MaxConcurrentTasks { get; set; } = 4;
    public int MaxQueueSize { get; set; } = 100;
    public int HighPriorityThreshold { get; set; } = 10;
    public int ProcessingTimeoutSeconds { get; set; } = 120;
}

public class StorageSettings
{
    public string DatabasePath { get; set; } = "ActivityData.db";
    public bool RetainFrames { get; set; } = false;
    public int CompactionIntervalHours { get; set; } = 24;
    public int MaxEventAgeDays { get; set; } = 90;
}
