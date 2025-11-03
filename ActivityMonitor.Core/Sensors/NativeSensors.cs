using System.Runtime.InteropServices;
using System.Text;

namespace ActivityMonitor.Core.Sensors;

/// <summary>
/// Provides low-level Win32 API access for activity monitoring
/// </summary>
public class NativeSensors
{
    #region Win32 API Declarations

    [DllImport("user32.dll")]
    private static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(IntPtr hWnd, out int processId);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);

    [DllImport("user32.dll")]
    private static extern bool GetLastInputInfo(ref LASTINPUTINFO plii);

    [DllImport("kernel32.dll")]
    private static extern uint GetTickCount();

    [StructLayout(LayoutKind.Sequential)]
    private struct LASTINPUTINFO
    {
        public uint cbSize;
        public uint dwTime;
    }

    #endregion

    /// <summary>
    /// Gets the handle of the currently focused window
    /// </summary>
    public IntPtr GetForegroundWindowHandle()
    {
        return GetForegroundWindow();
    }

    /// <summary>
    /// Gets the process ID associated with a window handle
    /// </summary>
    public int GetProcessIdFromWindow(IntPtr windowHandle)
    {
        GetWindowThreadProcessId(windowHandle, out int processId);
        return processId;
    }

    /// <summary>
    /// Gets the window title for a given window handle
    /// </summary>
    public string GetWindowTitle(IntPtr windowHandle)
    {
        const int maxLength = 256;
        var text = new StringBuilder(maxLength);
        int length = GetWindowText(windowHandle, text, maxLength);
        
        return length > 0 ? text.ToString() : string.Empty;
    }

    /// <summary>
    /// Gets the time elapsed since the last user input (keyboard or mouse)
    /// </summary>
    public TimeSpan GetIdleTime()
    {
        var lastInputInfo = new LASTINPUTINFO
        {
            cbSize = (uint)Marshal.SizeOf<LASTINPUTINFO>()
        };

        if (!GetLastInputInfo(ref lastInputInfo))
        {
            return TimeSpan.Zero;
        }

        uint currentTickCount = GetTickCount();
        uint idleTickCount = currentTickCount - lastInputInfo.dwTime;

        return TimeSpan.FromMilliseconds(idleTickCount);
    }

    /// <summary>
    /// Gets the timestamp of the last user input
    /// </summary>
    public DateTime GetLastInputTimestamp()
    {
        var idleTime = GetIdleTime();
        return DateTime.Now - idleTime;
    }
}
