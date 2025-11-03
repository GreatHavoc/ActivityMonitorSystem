using ActivityMonitor.Common.Configuration;
using ActivityMonitor.Common.Models;
using ActivityMonitor.Core.Sensors;
using ActivityMonitor.Core.Queue;
using ActivityMonitor.Core.Storage;
using ActivityMonitor.Core.Capture;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging;
using Microsoft.Extensions.Options;

namespace ActivityMonitor.Service;

public sealed class ActivityMonitorService : BackgroundService
{
    private readonly ILogger<ActivityMonitorService> _logger;
    private readonly ActivityMonitorSettings _settings;
    private readonly FocusTracker _focusTracker;
    private readonly IdleDetector _idleDetector;
    private readonly RequestQueueManager _queueManager;
    private readonly ActivityDatabase _database;
    private readonly CaptureManager _captureManager;
    private DateTime _lastCaptureTime = DateTime.MinValue;
    private DateTime _lastPeriodicCaptureTime = DateTime.MinValue;
    private string? _lastFocusedWindow;

    public ActivityMonitorService(
        ILogger<ActivityMonitorService> logger,
        IOptions<ActivityMonitorSettings> settings,
        FocusTracker focusTracker,
        IdleDetector idleDetector,
        RequestQueueManager queueManager,
        ActivityDatabase database,
        CaptureManager captureManager)
    {
        _logger = logger;
        _settings = settings.Value;
        _focusTracker = focusTracker;
        _idleDetector = idleDetector;
        _queueManager = queueManager;
        _database = database;
        _captureManager = captureManager;
    }

    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        try
        {
            _logger.LogInformation("Activity Monitor Service starting...");

            // Initialize database
            await _database.InitializeAsync(stoppingToken);

            // Start queue manager
            await _queueManager.StartAsync(stoppingToken);

            _logger.LogInformation("Activity Monitor Service started successfully");

            var samplingInterval = TimeSpan.FromSeconds(_settings.SamplingIntervalSeconds);
            var periodicCaptureInterval = TimeSpan.FromMinutes(3); // Capture every 3 minutes for continuous monitoring
            bool wasIdle = false;

            while (!stoppingToken.IsCancellationRequested)
            {
                try
                {
                    // Periodic capture for continuous activity tracking
                    if (DateTime.UtcNow - _lastPeriodicCaptureTime >= periodicCaptureInterval)
                    {
                        _logger.LogInformation("Triggering periodic capture for continuous monitoring");
                        await TriggerCaptureAsync("periodic_monitoring", RequestPriority.Normal, stoppingToken);
                        _lastPeriodicCaptureTime = DateTime.UtcNow;
                    }

                    // Check idle state
                    var idleInfo = _idleDetector.GetIdleState();
                    var isCurrentlyIdle = idleInfo.IsIdle;

                    // Detect idle state changes
                    if (wasIdle && !isCurrentlyIdle)
                    {
                        _logger.LogInformation("User resumed from idle");
                        
                        // Trigger capture on idle resume
                        if (_settings.CaptureSettings.TriggerOnIdleResume)
                        {
                            await TriggerCaptureAsync("idle_resume", RequestPriority.High, stoppingToken);
                        }
                    }

                    wasIdle = isCurrentlyIdle;

                    // Track active window if not idle
                    if (!isCurrentlyIdle)
                    {
                        var focusInfo = _focusTracker.GetCurrentFocus();
                        
                        if (focusInfo != null)
                        {
                            var windowKey = $"{focusInfo.ProcessId}:{focusInfo.WindowTitle}";

                            // Detect focus change
                            if (_lastFocusedWindow != windowKey)
                            {
                                _logger.LogInformation(
                                    "Focus changed to: {Process} - {Title}", 
                                    focusInfo.ProcessName, 
                                    focusInfo.WindowTitle);

                                // Record activity event
                                var activityEvent = new ActivityEvent
                                {
                                    Timestamp = DateTime.UtcNow,
                                    EventType = "focus_change",
                                    ProcessId = focusInfo.ProcessId,
                                    ProcessName = focusInfo.ProcessName,
                                    WindowTitle = focusInfo.WindowTitle,
                                    IsIdle = false
                                };

                                await _database.SaveActivityEventAsync(activityEvent, stoppingToken);

                                // Trigger capture on focus change
                                if (_settings.CaptureSettings.TriggerOnFocusChange)
                                {
                                    await TriggerCaptureAsync("focus_change", RequestPriority.High, stoppingToken);
                                }

                                _lastFocusedWindow = windowKey;
                            }
                        }
                    }
                    else
                    {
                        // Record idle event periodically
                        var idleEvent = new ActivityEvent
                        {
                            Timestamp = DateTime.UtcNow,
                            EventType = "idle",
                            IsIdle = true,
                            IdleDurationSeconds = (int)idleInfo.IdleDuration.TotalSeconds
                        };

                        await _database.SaveActivityEventAsync(idleEvent, stoppingToken);
                    }

                    await Task.Delay(samplingInterval, stoppingToken);
                }
                catch (OperationCanceledException)
                {
                    throw; // Re-throw to exit gracefully
                }
                catch (Exception ex)
                {
                    _logger.LogError(ex, "Error in activity monitoring loop");
                    await Task.Delay(TimeSpan.FromSeconds(5), stoppingToken);
                }
            }
        }
        catch (OperationCanceledException)
        {
            _logger.LogInformation("Activity Monitor Service stopping...");
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Fatal error in Activity Monitor Service");
            Environment.Exit(1);
        }
        finally
        {
            await _queueManager.StopAsync();
            _logger.LogInformation("Activity Monitor Service stopped");
        }
    }

    private async Task TriggerCaptureAsync(string triggerReason, RequestPriority priority, CancellationToken cancellationToken)
    {
        // Don't throttle periodic captures, only event-based ones
        if (triggerReason != "periodic_monitoring")
        {
            var minCaptureInterval = TimeSpan.FromSeconds(10); // Reduced from 30 to 10 seconds
            
            if (DateTime.UtcNow - _lastCaptureTime < minCaptureInterval)
            {
                _logger.LogDebug("Skipping capture, too soon since last capture");
                return;
            }
        }

        _logger.LogInformation("Triggering screen capture: {Reason}", triggerReason);

        var request = new InferenceRequest
        {
            Id = Guid.NewGuid(),
            Timestamp = DateTime.UtcNow,
            TriggerReason = triggerReason,
            Priority = priority
        };

        await _queueManager.EnqueueAsync(request, cancellationToken);
        
        if (triggerReason != "periodic_monitoring")
        {
            _lastCaptureTime = DateTime.UtcNow;
        }
    }

    public override async Task StopAsync(CancellationToken cancellationToken)
    {
        _logger.LogInformation("Activity Monitor Service is stopping");
        await base.StopAsync(cancellationToken);
    }
}
