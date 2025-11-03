using ActivityMonitor.Common.Models;
using Microsoft.Extensions.Logging;
using System.Diagnostics;

namespace ActivityMonitor.Core.Sensors;

/// <summary>
/// Tracks the currently focused window and process
/// </summary>
public class FocusTracker
{
    private readonly NativeSensors _sensors;
    private readonly ILogger<FocusTracker> _logger;

    public FocusTracker(NativeSensors sensors, ILogger<FocusTracker> logger)
    {
        _sensors = sensors;
        _logger = logger;
    }

    /// <summary>
    /// Gets information about the currently focused window
    /// </summary>
    public FocusInfo? GetCurrentFocus()
    {
        try
        {
            var windowHandle = _sensors.GetForegroundWindowHandle();
            
            if (windowHandle == IntPtr.Zero)
            {
                return null;
            }

            var processId = _sensors.GetProcessIdFromWindow(windowHandle);
            var windowTitle = _sensors.GetWindowTitle(windowHandle);

            if (processId == 0)
            {
                return null;
            }

            // Get process name
            string processName = string.Empty;
            try
            {
                using var process = Process.GetProcessById(processId);
                processName = process.ProcessName;
            }
            catch (Exception ex)
            {
                _logger.LogDebug(ex, "Could not get process name for PID {ProcessId}", processId);
                processName = "Unknown";
            }

            return new FocusInfo
            {
                WindowHandle = windowHandle,
                ProcessId = processId,
                ProcessName = processName,
                WindowTitle = windowTitle,
                Timestamp = DateTime.UtcNow
            };
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Error getting current focus information");
            return null;
        }
    }
}
