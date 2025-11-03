using ActivityMonitor.Common.Configuration;
using ActivityMonitor.Core.Queue;
using ActivityMonitor.Core.Sensors;
using ActivityMonitor.Core.Capture;
using ActivityMonitor.Core.Inference;
using ActivityMonitor.Core.Storage;
using Microsoft.Extensions.Configuration;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging;
using Serilog;

namespace ActivityMonitor.Service;

public class Program
{
    public static async Task<int> Main(string[] args)
    {
        // Create Serilog logger for early initialization
        Log.Logger = new LoggerConfiguration()
            .WriteTo.Console()
            .WriteTo.File(@"C:\ActivityMonitor\publish\Logs\activitymonitor-.log", rollingInterval: RollingInterval.Day)
            .CreateBootstrapLogger();

        try
        {
            Log.Information("Starting Activity Monitor");

            var builder = Host.CreateApplicationBuilder(args);

            // Only configure as Windows Service if running as service
            if (OperatingSystem.IsWindows() && args.Contains("--service"))
            {
                builder.Services.AddWindowsService(options =>
                {
                    options.ServiceName = "Activity Monitor Service";
                });
            }

            // Add Serilog
            builder.Services.AddSerilog((services, lc) => lc
                .ReadFrom.Configuration(builder.Configuration)
                .ReadFrom.Services(services)
                .Enrich.FromLogContext()
                .WriteTo.Console()
                .WriteTo.File(@"C:\ActivityMonitor\publish\Logs\activitymonitor-.log", 
                    rollingInterval: RollingInterval.Day,
                    retainedFileCountLimit: 30));

            // Configuration
            builder.Services.Configure<ActivityMonitorSettings>(
                builder.Configuration.GetSection("ActivityMonitor"));

            // Core Services
            builder.Services.AddSingleton<NativeSensors>();
            builder.Services.AddSingleton<FocusTracker>();
            builder.Services.AddSingleton<IdleDetector>();
            builder.Services.AddSingleton<RequestQueueManager>();
            builder.Services.AddSingleton<ActivityDatabase>();
            builder.Services.AddSingleton<CaptureManager>();
            builder.Services.AddHttpClient<OllamaInferenceClient>();

            // Background Service
            builder.Services.AddHostedService<ActivityMonitorService>();

            var host = builder.Build();
            await host.RunAsync();
            return 0;
        }
        catch (Exception ex)
        {
            Log.Fatal(ex, "Application terminated unexpectedly");
            return 1;
        }
        finally
        {
            await Log.CloseAndFlushAsync();
        }
    }
}
