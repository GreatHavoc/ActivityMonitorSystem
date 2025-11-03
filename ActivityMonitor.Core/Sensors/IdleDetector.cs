using ActivityMonitor.Common.Configuration;
using ActivityMonitor.Common.Models;
using Microsoft.Extensions.Logging;
using Microsoft.Extensions.Options;

namespace ActivityMonitor.Core.Sensors;

/// <summary>
/// Detects user idle state based on last input time
/// </summary>
public class IdleDetector
{
    private readonly NativeSensors _sensors;
    private readonly ILogger<IdleDetector> _logger;
    private readonly ActivityMonitorSettings _settings;

    public IdleDetector(
        NativeSensors sensors, 
        ILogger<IdleDetector> logger,
        IOptions<ActivityMonitorSettings> settings)
    {
        _sensors = sensors;
        _logger = logger;
        _settings = settings.Value;
    }

    /// <summary>
    /// Gets the current idle state of the system
    /// </summary>
    public IdleInfo GetIdleState()
    {
        try
        {
            var idleDuration = _sensors.GetIdleTime();
            var idleThreshold = TimeSpan.FromSeconds(_settings.IdleThresholdSeconds);
            var lastInputTime = _sensors.GetLastInputTimestamp();

            var isIdle = idleDuration >= idleThreshold;

            return new IdleInfo
            {
                IsIdle = isIdle,
                IdleDuration = idleDuration,
                LastInputTime = lastInputTime
            };
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Error detecting idle state");
            
            // Return non-idle state on error (safe default)
            return new IdleInfo
            {
                IsIdle = false,
                IdleDuration = TimeSpan.Zero,
                LastInputTime = DateTime.Now
            };
        }
    }
}
