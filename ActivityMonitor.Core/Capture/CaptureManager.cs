using ActivityMonitor.Common.Configuration;
using Microsoft.Extensions.Logging;
using Microsoft.Extensions.Options;
using System.Drawing;
using System.Drawing.Imaging;

namespace ActivityMonitor.Core.Capture;

/// <summary>
/// Manages screen capture using Windows Graphics Capture API
/// Captures low-FPS screen clips for analysis
/// </summary>
public class CaptureManager
{
    private readonly ILogger<CaptureManager> _logger;
    private readonly ActivityMonitorSettings _settings;
    private readonly SemaphoreSlim _captureLock = new(1, 1);

    public CaptureManager(
        ILogger<CaptureManager> logger,
        IOptions<ActivityMonitorSettings> settings)
    {
        _logger = logger;
        _settings = settings.Value;
    }

    /// <summary>
    /// Captures a series of screen frames for analysis
    /// </summary>
    public async Task<List<byte[]>?> CaptureFramesAsync(CancellationToken cancellationToken)
    {
        // Ensure only one capture happens at a time
        if (!await _captureLock.WaitAsync(0, cancellationToken))
        {
            _logger.LogWarning("Capture already in progress, skipping");
            return null;
        }

        try
        {
            var frames = new List<byte[]>();
            var captureSettings = _settings.CaptureSettings;
            var frameInterval = TimeSpan.FromSeconds(1.0 / captureSettings.FrameRate);
            var maxFrames = Math.Min(
                captureSettings.MaxFramesPerCapture,
                captureSettings.MaxDurationSeconds * captureSettings.FrameRate);

            _logger.LogInformation("Starting capture: {MaxFrames} frames at {FrameRate} FPS", 
                maxFrames, captureSettings.FrameRate);

            for (int i = 0; i < maxFrames && !cancellationToken.IsCancellationRequested; i++)
            {
                try
                {
                    var frameData = CaptureScreen();
                    
                    if (frameData != null)
                    {
                        frames.Add(frameData);
                        _logger.LogDebug("Captured frame {FrameNumber}/{MaxFrames}", i + 1, maxFrames);
                    }

                    // Wait for next frame
                    if (i < maxFrames - 1)
                    {
                        await Task.Delay(frameInterval, cancellationToken);
                    }
                }
                catch (Exception ex)
                {
                    _logger.LogError(ex, "Error capturing frame {FrameNumber}", i + 1);
                }
            }

            _logger.LogInformation("Capture completed: {FrameCount} frames captured", frames.Count);
            return frames.Count > 0 ? frames : null;
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Error during screen capture");
            return null;
        }
        finally
        {
            _captureLock.Release();
        }
    }

    /// <summary>
    /// Captures the current screen as a byte array (JPEG format)
    /// Uses System.Drawing for now - can be enhanced with Windows.Graphics.Capture API
    /// </summary>
    private byte[]? CaptureScreen()
    {
        try
        {
            var bounds = System.Windows.Forms.Screen.PrimaryScreen?.Bounds;
            
            if (bounds == null)
            {
                return null;
            }

            using var bitmap = new Bitmap(bounds.Value.Width, bounds.Value.Height);
            using var graphics = Graphics.FromImage(bitmap);
            
            graphics.CopyFromScreen(
                bounds.Value.X, 
                bounds.Value.Y, 
                0, 
                0, 
                bounds.Value.Size);

            using var stream = new MemoryStream();
            
            // Compress to JPEG for smaller size
            var jpegEncoder = GetEncoder(ImageFormat.Jpeg);
            var encoderParameters = new EncoderParameters(1);
            encoderParameters.Param[0] = new EncoderParameter(
                System.Drawing.Imaging.Encoder.Quality, 
                75L); // 75% quality

            bitmap.Save(stream, jpegEncoder, encoderParameters);
            return stream.ToArray();
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Error capturing screen");
            return null;
        }
    }

    private ImageCodecInfo GetEncoder(ImageFormat format)
    {
        var codecs = ImageCodecInfo.GetImageEncoders();
        return codecs.First(codec => codec.FormatID == format.Guid);
    }
}
