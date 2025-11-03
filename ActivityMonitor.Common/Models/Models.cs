namespace ActivityMonitor.Common.Models;

public class ActivityEvent
{
    public long Id { get; set; }
    public DateTime Timestamp { get; set; }
    public string EventType { get; set; } = string.Empty;
    public int? ProcessId { get; set; }
    public string? ProcessName { get; set; }
    public string? WindowTitle { get; set; }
    public bool IsIdle { get; set; }
    public int? IdleDurationSeconds { get; set; }
    public string? Metadata { get; set; }
}

public class FocusInfo
{
    public IntPtr WindowHandle { get; set; }
    public int ProcessId { get; set; }
    public string ProcessName { get; set; } = string.Empty;
    public string WindowTitle { get; set; } = string.Empty;
    public DateTime Timestamp { get; set; }
}

public class IdleInfo
{
    public bool IsIdle { get; set; }
    public TimeSpan IdleDuration { get; set; }
    public DateTime LastInputTime { get; set; }
}

public enum RequestPriority
{
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3
}

public class InferenceRequest
{
    public Guid Id { get; set; }
    public DateTime Timestamp { get; set; }
    public string TriggerReason { get; set; } = string.Empty;
    public RequestPriority Priority { get; set; }
    public List<byte[]>? CapturedFrames { get; set; }
    public string? Metadata { get; set; }
}

public class InferenceResult
{
    public Guid RequestId { get; set; }
    public DateTime ProcessedAt { get; set; }
    public string ActivityLabel { get; set; } = string.Empty;
    public string Application { get; set; } = string.Empty;
    public string ContentType { get; set; } = string.Empty;
    public string Topic { get; set; } = string.Empty;
    public string Action { get; set; } = string.Empty;
    public string Summary { get; set; } = string.Empty;
    public string VisibleText { get; set; } = string.Empty;
    public List<DetectedObject>? DetectedObjects { get; set; }
    public double Confidence { get; set; }
    public string? RawResponse { get; set; }
}

public class DetectedObject
{
    public string Label { get; set; } = string.Empty;
    public double Confidence { get; set; }
    public BoundingBox? BoundingBox { get; set; }
}

public class BoundingBox
{
    public int X { get; set; }
    public int Y { get; set; }
    public int Width { get; set; }
    public int Height { get; set; }
}

public class QueueMetrics
{
    public int QueuedRequests { get; set; }
    public int ProcessingRequests { get; set; }
    public int CompletedRequests { get; set; }
    public int FailedRequests { get; set; }
    public TimeSpan AverageProcessingTime { get; set; }
    public DateTime LastUpdated { get; set; }
}
