using Microsoft.Data.Sqlite;
using Spectre.Console;
using System.Globalization;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Text.RegularExpressions;
using System.Linq;

namespace ActivityMonitor.CLI;

class Program
{
    static async Task<int> Main(string[] args)
    {
        AnsiConsole.Write(
            new FigletText("Activity Monitor")
                .Centered()
                .Color(Color.Blue));

        if (args.Length == 0)
        {
            ShowHelp();
            return 0;
        }

        var command = args[0].ToLower();

        try
        {
            return command switch
            {
                "query" => await HandleQueryCommand(args),
                "stats" => await HandleStatsCommand(),
                "timeline" => await HandleTimelineCommand(args),
                "detailed" => await HandleDetailedCommand(args),
                "summary" => await HandleSummaryCommand(args),
                "report" or "export" => await HandleReportCommand(args),
                "help" or "--help" or "-h" => ShowHelp(),
                _ => ShowHelp()
            };
        }
        catch (Exception ex)
        {
            AnsiConsole.MarkupLine($"[red]Error: {ex.Message}[/]");
            return 1;
        }
    }

    static int ShowHelp()
    {
        AnsiConsole.MarkupLine("[yellow]Usage:[/]");
        AnsiConsole.MarkupLine("  ActivityMonitor.CLI [command] [options]");
        AnsiConsole.WriteLine();
        AnsiConsole.MarkupLine("[yellow]Commands:[/]");
        AnsiConsole.MarkupLine("  [cyan]timeline[/]   Show comprehensive timeline of all activities");
        AnsiConsole.MarkupLine("               Options: --from <date> --to <date>");
        AnsiConsole.MarkupLine("               Example: ActivityMonitor.CLI timeline --from \"2025-10-12\"");
        AnsiConsole.WriteLine();
        AnsiConsole.MarkupLine("  [cyan]detailed[/]   Show detailed activity logs with AI analysis");
        AnsiConsole.MarkupLine("               Options: --from <date> --to <date> --limit <number>");
        AnsiConsole.MarkupLine("               Example: ActivityMonitor.CLI detailed --from \"2025-10-12\" --limit 50");
        AnsiConsole.WriteLine();
        AnsiConsole.MarkupLine("  [cyan]summary[/]    Show daily summary of activities by category");
        AnsiConsole.MarkupLine("               Options: --date <date>");
        AnsiConsole.MarkupLine("               Example: ActivityMonitor.CLI summary --date \"2025-10-12\"");
        AnsiConsole.WriteLine();
        AnsiConsole.MarkupLine("  [cyan]report[/]     Export a detailed activity report to disk");
        AnsiConsole.MarkupLine("               Options: --from <date> --to <date> --output <path> --llm");
        AnsiConsole.MarkupLine("               --llm generates a compact LLM-optimized report with narratives");
        AnsiConsole.MarkupLine("               Example: ActivityMonitor.CLI report --from \"2025-10-22\" --output report.json");
        AnsiConsole.MarkupLine("               Example: ActivityMonitor.CLI report --from \"2025-10-22\" --llm");
        AnsiConsole.WriteLine();
        AnsiConsole.MarkupLine("  [cyan]query[/]      Query activity events (raw data)");
        AnsiConsole.MarkupLine("               Options: --from <date> --to <date> --limit <number>");
        AnsiConsole.WriteLine();
        AnsiConsole.MarkupLine("  [cyan]stats[/]      Show activity statistics");
        AnsiConsole.WriteLine();
        AnsiConsole.MarkupLine("  [cyan]help[/]       Show this help message");
        return 0;
    }

    static async Task<int> HandleQueryCommand(string[] args)
    {
        string? fromDate = null;
        string? toDate = null;
        int limit = 100;

        for (int i = 1; i < args.Length; i++)
        {
            if (args[i] == "--from" && i + 1 < args.Length)
                fromDate = args[++i];
            else if (args[i] == "--to" && i + 1 < args.Length)
                toDate = args[++i];
            else if (args[i] == "--limit" && i + 1 < args.Length)
                int.TryParse(args[++i], out limit);
        }

        await QueryEventsAsync(fromDate, toDate, limit);
        return 0;
    }

    static async Task<int> HandleStatsCommand()
    {
        await ShowStatsAsync();
        return 0;
    }

    static async Task QueryEventsAsync(string? fromDate, string? toDate, int limit)
    {
        var dbPath = GetDatabasePath();
        if (dbPath == null) return;

        AnsiConsole.MarkupLine($"[green]Using database: {EscapeMarkup(dbPath)}[/]");

        var from = string.IsNullOrEmpty(fromDate) 
            ? DateTime.Today 
            : DateTime.Parse(fromDate);
        var to = string.IsNullOrEmpty(toDate) 
            ? DateTime.Now 
            : DateTime.Parse(toDate);

        var connectionString = $"Data Source={dbPath}";

        using var connection = new SqliteConnection(connectionString);
        await connection.OpenAsync();

        var sql = @"
            SELECT Timestamp, EventType, ProcessName, WindowTitle, IsIdle
            FROM ActivityEvents
            WHERE Timestamp BETWEEN @From AND @To
            ORDER BY Timestamp DESC
            LIMIT @Limit
        ";

        using var command = new SqliteCommand(sql, connection);
        command.Parameters.AddWithValue("@From", from.ToString("O"));
        command.Parameters.AddWithValue("@To", to.ToString("O"));
        command.Parameters.AddWithValue("@Limit", limit);

        var table = new Table();
        table.AddColumn("Timestamp");
        table.AddColumn("Event Type");
        table.AddColumn("Process");
        table.AddColumn("Window Title");
        table.AddColumn("Status");

        using var reader = await command.ExecuteReaderAsync();
        
        while (await reader.ReadAsync())
        {
            var timestamp = DateTime.Parse(reader.GetString(0));
            var eventType = reader.GetString(1);
            var processName = reader.IsDBNull(2) ? "-" : reader.GetString(2);
            var windowTitle = reader.IsDBNull(3) ? "-" : reader.GetString(3);
            var isIdle = reader.GetInt32(4) == 1;

            var timestampText = EscapeMarkup(timestamp.ToLocalTime().ToString("yyyy-MM-dd HH:mm:ss"));
            var eventTypeText = EscapeMarkup(eventType);
            var processText = EscapeMarkup(processName);
            var window = windowTitle.Length > 50 ? windowTitle.Substring(0, 47) + "..." : windowTitle;
            var windowText = EscapeMarkup(window);
            var status = isIdle ? "[yellow]Idle[/]" : "[green]Active[/]";

            table.AddRow(
                timestampText,
                eventTypeText,
                processText,
                windowText,
                status
            );
        }

        AnsiConsole.Write(table);
    }

    static async Task ShowStatsAsync()
    {
        var dbPath = GetDatabasePath();
        if (dbPath == null) return;

        AnsiConsole.MarkupLine($"[green]Using database: {EscapeMarkup(dbPath)}[/]");

        var connectionString = $"Data Source={dbPath}";

        using var connection = new SqliteConnection(connectionString);
        await connection.OpenAsync();

        // Total events
        var totalEvents = await GetCountAsync(connection, "SELECT COUNT(*) FROM ActivityEvents");
        var totalInferences = await GetCountAsync(connection, "SELECT COUNT(*) FROM InferenceResults");

        // Events today
        var today = DateTime.Today;
        var eventsToday = await GetCountAsync(
            connection, 
            $"SELECT COUNT(*) FROM ActivityEvents WHERE Timestamp >= '{today:O}'");

        // Top processes
        var topProcessSql = @"
            SELECT ProcessName, COUNT(*) as Count
            FROM ActivityEvents
            WHERE ProcessName IS NOT NULL AND Timestamp >= @Since
            GROUP BY ProcessName
            ORDER BY Count DESC
            LIMIT 10
        ";

        AnsiConsole.Write(new Rule("[yellow]Activity Monitor Statistics[/]"));
        AnsiConsole.WriteLine();

        var table = new Table();
        table.AddColumn("Metric");
        table.AddColumn("Value");

        table.AddRow("Total Events", totalEvents.ToString("N0"));
        table.AddRow("Total Inferences", totalInferences.ToString("N0"));
        table.AddRow("Events Today", eventsToday.ToString("N0"));

        AnsiConsole.Write(table);

        AnsiConsole.WriteLine();
        AnsiConsole.Write(new Rule("[yellow]Top Processes (Last 7 Days)[/]"));
        AnsiConsole.WriteLine();

        var processTable = new Table();
        processTable.AddColumn("Process");
        processTable.AddColumn("Event Count");

        using var command = new SqliteCommand(topProcessSql, connection);
        command.Parameters.AddWithValue("@Since", DateTime.Now.AddDays(-7).ToString("O"));

        using var reader = await command.ExecuteReaderAsync();
        
        while (await reader.ReadAsync())
        {
            var processName = EscapeMarkup(reader.GetString(0));
            var count = EscapeMarkup(reader.GetInt32(1).ToString("N0"));
            processTable.AddRow(processName, count);
        }

        AnsiConsole.Write(processTable);
    }

    static async Task<long> GetCountAsync(SqliteConnection connection, string sql)
    {
        using var command = new SqliteCommand(sql, connection);
        var result = await command.ExecuteScalarAsync();
        return Convert.ToInt64(result);
    }

    static async Task<int> HandleTimelineCommand(string[] args)
    {
        string? fromDate = null;
        string? toDate = null;

        for (int i = 1; i < args.Length; i++)
        {
            if (args[i] == "--from" && i + 1 < args.Length)
                fromDate = args[++i];
            else if (args[i] == "--to" && i + 1 < args.Length)
                toDate = args[++i];
        }

        await ShowTimelineAsync(fromDate, toDate);
        return 0;
    }

    static async Task<int> HandleDetailedCommand(string[] args)
    {
        string? fromDate = null;
        string? toDate = null;
        int limit = 100;

        for (int i = 1; i < args.Length; i++)
        {
            if (args[i] == "--from" && i + 1 < args.Length)
                fromDate = args[++i];
            else if (args[i] == "--to" && i + 1 < args.Length)
                toDate = args[++i];
            else if (args[i] == "--limit" && i + 1 < args.Length)
                int.TryParse(args[++i], out limit);
        }

        await ShowDetailedActivityAsync(fromDate, toDate, limit);
        return 0;
    }

    static async Task<int> HandleSummaryCommand(string[] args)
    {
        string? date = null;

        for (int i = 1; i < args.Length; i++)
        {
            if (args[i] == "--date" && i + 1 < args.Length)
                date = args[++i];
        }

        await ShowDailySummaryAsync(date);
        return 0;
    }

    static async Task<int> HandleReportCommand(string[] args)
    {
        string? fromDate = null;
        string? toDate = null;
        string? outputPath = null;
        string format = "json";
        bool llmFormat = false;

        for (int i = 1; i < args.Length; i++)
        {
            switch (args[i])
            {
                case "--from" when i + 1 < args.Length:
                    fromDate = args[++i];
                    break;
                case "--to" when i + 1 < args.Length:
                    toDate = args[++i];
                    break;
                case "--output" when i + 1 < args.Length:
                    outputPath = args[++i];
                    break;
                case "--format" when i + 1 < args.Length:
                    format = args[++i];
                    break;
                case "--llm":
                    llmFormat = true;
                    break;
            }
        }

        var dbPath = GetDatabasePath();
        if (dbPath == null)
        {
            return 1;
        }

        var fromLocal = string.IsNullOrEmpty(fromDate) ? DateTime.Today : DateTime.Parse(fromDate);
        var toLocalInclusive = string.IsNullOrEmpty(toDate) ? fromLocal : DateTime.Parse(toDate);

        if (toLocalInclusive < fromLocal)
        {
            (fromLocal, toLocalInclusive) = (toLocalInclusive, fromLocal);
        }

        var normalizedFormat = string.IsNullOrWhiteSpace(format) ? "json" : format.Trim().ToLowerInvariant();
        if (normalizedFormat != "json")
        {
            AnsiConsole.MarkupLine("[yellow]Only JSON export is currently supported. Defaulting to JSON.[/]");
            normalizedFormat = "json";
        }

        if (string.IsNullOrWhiteSpace(outputPath))
        {
            var fileNameBuilder = new StringBuilder("activity-report-");
            if (llmFormat)
            {
                fileNameBuilder.Append("llm-");
            }
            fileNameBuilder.Append(fromLocal.ToString("yyyyMMdd"));
            if (toLocalInclusive != fromLocal)
            {
                fileNameBuilder.Append('-').Append(toLocalInclusive.ToString("yyyyMMdd"));
            }
            fileNameBuilder.Append('-').Append(DateTime.Now.ToString("HHmmss"));
            fileNameBuilder.Append('.').Append(normalizedFormat);
            outputPath = Path.Combine(Directory.GetCurrentDirectory(), fileNameBuilder.ToString());
        }

        if (!Path.IsPathRooted(outputPath))
        {
            outputPath = Path.GetFullPath(Path.Combine(Directory.GetCurrentDirectory(), outputPath));
        }

        var outputDirectory = Path.GetDirectoryName(outputPath);
        if (!string.IsNullOrEmpty(outputDirectory) && !Directory.Exists(outputDirectory))
        {
            Directory.CreateDirectory(outputDirectory);
        }

        // Handle LLM format separately
        if (llmFormat)
        {
            var llmReport = await GenerateLlmReportAsync(dbPath, fromLocal, toLocalInclusive, outputPath);
            AnsiConsole.MarkupLine($"[green]LLM-optimized report exported to {EscapeMarkup(outputPath)}[/]");
            AnsiConsole.MarkupLine($"[cyan]Schema:[/] {llmReport.SchemaVersion}");
            if (llmReport.ExecutiveSummary != null)
            {
                AnsiConsole.MarkupLine($"[cyan]Summary:[/] {EscapeMarkup(llmReport.ExecutiveSummary.OneLiner)}");
                AnsiConsole.MarkupLine($"[dim]Projects: {llmReport.Projects?.Count ?? 0} | Insights: {llmReport.ExecutiveSummary.TotalInsights} | Moments: {llmReport.SignificantMoments?.Count ?? 0}[/]");
            }
            return 0;
        }

        var report = await ExportActivityReportAsync(dbPath, fromLocal, toLocalInclusive, outputPath);

        AnsiConsole.MarkupLine($"[green]Report exported to {EscapeMarkup(outputPath)}[/]");
        
        // Display check-in/check-out summary
        if (!string.IsNullOrEmpty(report.CheckInTimeUtc) && !string.IsNullOrEmpty(report.CheckOutTimeUtc))
        {
            var checkIn = DateTime.Parse(report.CheckInTimeUtc).ToLocalTime();
            var checkOut = DateTime.Parse(report.CheckOutTimeUtc).ToLocalTime();
            AnsiConsole.MarkupLine($"[cyan]Check-in:[/] {checkIn:HH:mm:ss}  [cyan]Check-out:[/] {checkOut:HH:mm:ss}");
        }
        else
        {
            AnsiConsole.MarkupLine("[yellow]No activity events found in the specified date range.[/]");
        }
        
        AnsiConsole.MarkupLine($"[dim]Tracked time: {FormatDuration(report.TotalTrackedSeconds)} (Active: {FormatDuration(report.TotalActiveSeconds)}, Idle: {FormatDuration(report.TotalIdleSeconds)})[/]");
        return 0;
    }

    static async Task ShowTimelineAsync(string? fromDate, string? toDate)
    {
        var dbPath = GetDatabasePath();
        if (dbPath == null) return;

        AnsiConsole.MarkupLine($"[green]Using database: {EscapeMarkup(dbPath)}[/]");
        AnsiConsole.WriteLine();

        var from = string.IsNullOrEmpty(fromDate) ? DateTime.Today : DateTime.Parse(fromDate);
        var to = string.IsNullOrEmpty(toDate) ? DateTime.Now : DateTime.Parse(toDate);

        var connectionString = $"Data Source={dbPath}";

        using var connection = new SqliteConnection(connectionString);
        await connection.OpenAsync();

        // Query both activity events and inference results
        var sql = @"
            SELECT 
                ae.Timestamp,
                ae.EventType,
                ae.ProcessName,
                ae.WindowTitle,
                ae.IsIdle,
                ir.ActivityLabel,
                ir.Application,
                ir.ContentType,
                ir.Topic,
                ir.Action,
                ir.Summary,
                ir.VisibleText
            FROM ActivityEvents ae
            LEFT JOIN InferenceResults ir ON DATE(ae.Timestamp) = DATE(ir.ProcessedAt)
                AND ABS((JULIANDAY(ae.Timestamp) - JULIANDAY(ir.ProcessedAt)) * 86400.0) < 60
            WHERE ae.Timestamp BETWEEN @From AND @To
            ORDER BY ae.Timestamp DESC
            LIMIT 500
        ";

        using var command = new SqliteCommand(sql, connection);
        command.Parameters.AddWithValue("@From", from.ToString("O"));
        command.Parameters.AddWithValue("@To", to.ToString("O"));

        var timeline = new List<TimelineEntry>();

        using var reader = await command.ExecuteReaderAsync();
        while (await reader.ReadAsync())
        {
            timeline.Add(new TimelineEntry
            {
                Timestamp = DateTime.Parse(reader.GetString(0)),
                EventType = reader.GetString(1),
                ProcessName = reader.IsDBNull(2) ? null : reader.GetString(2),
                WindowTitle = reader.IsDBNull(3) ? null : reader.GetString(3),
                IsIdle = reader.GetInt32(4) == 1,
                ActivityLabel = reader.IsDBNull(5) ? null : reader.GetString(5),
                Application = reader.IsDBNull(6) ? null : reader.GetString(6),
                ContentType = reader.IsDBNull(7) ? null : reader.GetString(7),
                Topic = reader.IsDBNull(8) ? null : reader.GetString(8),
                Action = reader.IsDBNull(9) ? null : reader.GetString(9),
                Summary = reader.IsDBNull(10) ? null : reader.GetString(10),
                VisibleText = reader.IsDBNull(11) ? null : reader.GetString(11)
            });
        }

        AnsiConsole.Write(new Rule($"[yellow]Activity Timeline: {from:yyyy-MM-dd} to {to:yyyy-MM-dd}[/]"));
        AnsiConsole.WriteLine();

        foreach (var entry in timeline.OrderBy(e => e.Timestamp))
        {
            var time = entry.Timestamp.ToLocalTime().ToString("HH:mm:ss");
            
            if (entry.IsIdle)
            {
                AnsiConsole.MarkupLine($"[dim]{time}[/] [yellow]● IDLE[/]");
            }
            else if (!string.IsNullOrEmpty(entry.ActivityLabel))
            {
                var activityLabel = EscapeMarkup(entry.ActivityLabel);
                AnsiConsole.MarkupLine($"[dim]{time}[/] [cyan]● {activityLabel}[/]");
                if (!string.IsNullOrEmpty(entry.Application))
                {
                    var application = EscapeMarkup(entry.Application);
                    AnsiConsole.MarkupLine($"         [dim]App:[/] {application}");
                }
                if (!string.IsNullOrEmpty(entry.Topic))
                {
                    var topic = EscapeMarkup(entry.Topic);
                    AnsiConsole.MarkupLine($"         [dim]Topic:[/] {topic}");
                }
                if (!string.IsNullOrEmpty(entry.Action))
                {
                    var action = EscapeMarkup(entry.Action);
                    AnsiConsole.MarkupLine($"         [dim]Action:[/] {action}");
                }
                if (!string.IsNullOrEmpty(entry.Summary))
                {
                    var shortSummary = entry.Summary.Length > 100 
                        ? entry.Summary.Substring(0, 97) + "..." 
                        : entry.Summary;
                    var summary = EscapeMarkup(shortSummary);
                    AnsiConsole.MarkupLine($"         [dim]Summary:[/] {summary}");
                }
                if (!string.IsNullOrEmpty(entry.VisibleText))
                {
                    var shortText = entry.VisibleText.Length > 80
                        ? entry.VisibleText.Substring(0, 77) + "..."
                        : entry.VisibleText;
                    var visibleText = EscapeMarkup(shortText);
                    AnsiConsole.MarkupLine($"         [dim]Text:[/] [grey]{visibleText}[/]");
                }
            }
            else
            {
                var processInfo = EscapeMarkup(entry.ProcessName ?? "Unknown");
                var windowInfo = entry.WindowTitle ?? string.Empty;
                if (windowInfo.Length > 50)
                    windowInfo = windowInfo.Substring(0, 47) + "...";
                var window = EscapeMarkup(windowInfo);
                AnsiConsole.MarkupLine($"[dim]{time}[/] [green]● {processInfo}[/] [dim]{window}[/]");
            }
            
            AnsiConsole.WriteLine();
        }

        AnsiConsole.MarkupLine($"[green]Total activities: {timeline.Count}[/]");
    }

    static async Task<ActivityReport> ExportActivityReportAsync(string dbPath, DateTime fromLocal, DateTime toLocalInclusive, string outputPath)
    {
        var fromUtc = DateTime.SpecifyKind(fromLocal, DateTimeKind.Local).ToUniversalTime();
        var toUtcExclusive = DateTime.SpecifyKind(toLocalInclusive, DateTimeKind.Local).AddDays(1).ToUniversalTime();

        var connectionString = $"Data Source={dbPath}";

        using var connection = new SqliteConnection(connectionString);
        await connection.OpenAsync();

        EventSnapshot? previousEvent = null;
        const string previousSql = @"
            SELECT Timestamp, EventType, ProcessName, WindowTitle, IsIdle
            FROM ActivityEvents
            WHERE Timestamp < @From
            ORDER BY Timestamp DESC
            LIMIT 1
        ";

        using (var previousCommand = new SqliteCommand(previousSql, connection))
        {
            previousCommand.Parameters.AddWithValue("@From", fromUtc.ToString("O"));

            using var reader = await previousCommand.ExecuteReaderAsync();
            if (await reader.ReadAsync())
            {
                previousEvent = new EventSnapshot
                {
                    TimestampUtc = ParseUtc(reader.GetString(0)),
                    EventType = reader.GetString(1),
                    ProcessName = reader.IsDBNull(2) ? null : reader.GetString(2),
                    WindowTitle = reader.IsDBNull(3) ? null : reader.GetString(3),
                    IsIdle = reader.GetInt32(4) == 1
                };
            }
        }

        var events = new List<EventSnapshot>();
        const string eventsSql = @"
            SELECT Timestamp, EventType, ProcessName, WindowTitle, IsIdle
            FROM ActivityEvents
            WHERE Timestamp BETWEEN @From AND @To
            ORDER BY Timestamp ASC
        ";

        using (var eventCommand = new SqliteCommand(eventsSql, connection))
        {
            eventCommand.Parameters.AddWithValue("@From", fromUtc.ToString("O"));
            eventCommand.Parameters.AddWithValue("@To", toUtcExclusive.ToString("O"));

            using var reader = await eventCommand.ExecuteReaderAsync();
            while (await reader.ReadAsync())
            {
                events.Add(new EventSnapshot
                {
                    TimestampUtc = ParseUtc(reader.GetString(0)),
                    EventType = reader.GetString(1),
                    ProcessName = reader.IsDBNull(2) ? null : reader.GetString(2),
                    WindowTitle = reader.IsDBNull(3) ? null : reader.GetString(3),
                    IsIdle = reader.GetInt32(4) == 1
                });
            }
        }

        var segments = new List<TimelineSegment>();
        var usageBuilders = new Dictionary<string, ApplicationUsageBuilder>(StringComparer.OrdinalIgnoreCase);
        long totalActiveSeconds = 0;
        long totalIdleSeconds = 0;

        // Determine check-in (first event) and check-out (last event) times
        DateTime? checkInTime = events.Count > 0 ? events[0].TimestampUtc : null;
        DateTime? checkOutTime = events.Count > 0 ? events[^1].TimestampUtc : null;
        
        // TotalTrackedSeconds is now based on actual activity window (check-out - check-in)
        // If no events, tracked time is 0
        var durationSecondsTotal = (checkInTime.HasValue && checkOutTime.HasValue)
            ? (long)Math.Max(0, (checkOutTime.Value - checkInTime.Value).TotalSeconds)
            : 0L;

        // Use check-in time as segment start, or fall back to fromUtc if no events
        DateTime segmentStart = checkInTime ?? fromUtc;
        var currentEvent = (events.Count > 0) ? null : (previousEvent ?? new EventSnapshot
        {
            TimestampUtc = fromUtc,
            EventType = "idle",
            IsIdle = true
        });

        void Accumulate(EventSnapshot snapshot, DateTime startUtc, DateTime endUtc)
        {
            if (endUtc <= startUtc)
            {
                return;
            }

            var exactDuration = endUtc - startUtc;
            var durationSeconds = (long)exactDuration.TotalSeconds; // Truncate instead of round
            if (durationSeconds <= 0)
            {
                return;
            }

            var segment = new TimelineSegment
            {
                EndUtc = endUtc.ToString("O"),
                DurationSeconds = durationSeconds,
                IsIdle = snapshot.IsIdle,
                ProcessName = snapshot.IsIdle
                    ? "Idle"
                    : (string.IsNullOrWhiteSpace(snapshot.ProcessName) ? "Unknown" : snapshot.ProcessName),
                WindowTitle = snapshot.IsIdle
                    ? null
                    : (string.IsNullOrWhiteSpace(snapshot.WindowTitle) ? null : snapshot.WindowTitle)
            };

            segments.Add(segment);

            if (snapshot.IsIdle)
            {
                totalIdleSeconds += durationSeconds;
                return;
            }

            var processKey = string.IsNullOrWhiteSpace(snapshot.ProcessName) ? "Unknown" : snapshot.ProcessName;
            if (!usageBuilders.TryGetValue(processKey, out var builder))
            {
                builder = new ApplicationUsageBuilder(processKey);
                usageBuilders[processKey] = builder;
            }

            builder.AddDuration(durationSeconds, snapshot.WindowTitle);
            totalActiveSeconds += durationSeconds;
        }

        foreach (var evt in events)
        {
            var timestamp = evt.TimestampUtc;
            // Clamp timestamps to the check-in/check-out window
            if (checkInTime.HasValue && timestamp < checkInTime.Value)
            {
                timestamp = checkInTime.Value;
            }
            if (checkOutTime.HasValue && timestamp > checkOutTime.Value)
            {
                timestamp = checkOutTime.Value;
            }

            if (currentEvent != null)
            {
                Accumulate(currentEvent, segmentStart, timestamp);
            }

            currentEvent = evt;
            segmentStart = timestamp;
        }

        // Note: We don't extend the last event to end of day anymore.
        // The last event's timestamp IS the check-out time, so no additional
        // accumulation is needed. Time between events is properly tracked,
        // and TotalTrackedSeconds = CheckOutTime - CheckInTime.

        // Ensure totals add up exactly to TotalTrackedSeconds
        // Any remaining time (tracked - active) is considered idle
        if (durationSecondsTotal > 0)
        {
            totalIdleSeconds = Math.Max(0, durationSecondsTotal - totalActiveSeconds);
        }

        var insights = new List<ActivityInsight>();
        var insightsByApplication = new Dictionary<string, List<ActivityInsight>>(StringComparer.OrdinalIgnoreCase);
        var contentSummaries = new Dictionary<string, ContentTypeSummaryBuilder>(StringComparer.OrdinalIgnoreCase);

        const string insightsSql = @"
            SELECT 
                ProcessedAt,
                ActivityLabel,
                Application,
                ContentType,
                Topic,
                Action,
                Summary,
                VisibleText,
                Confidence
            FROM InferenceResults
            WHERE ProcessedAt BETWEEN @From AND @To
            ORDER BY ProcessedAt ASC
        ";

        using (var insightCommand = new SqliteCommand(insightsSql, connection))
        {
            insightCommand.Parameters.AddWithValue("@From", fromUtc.ToString("O"));
            insightCommand.Parameters.AddWithValue("@To", toUtcExclusive.ToString("O"));

            using var reader = await insightCommand.ExecuteReaderAsync();
            while (await reader.ReadAsync())
            {
                var processedAt = ParseUtc(reader.GetString(0));

                var application = reader.IsDBNull(2) ? "Unknown" : reader.GetString(2);
                if (string.IsNullOrWhiteSpace(application))
                {
                    application = "Unknown";
                }

                var contentType = reader.IsDBNull(3) ? "Unknown" : reader.GetString(3);
                if (string.IsNullOrWhiteSpace(contentType))
                {
                    contentType = "Unknown";
                }

                var visibleText = reader.IsDBNull(7) ? string.Empty : reader.GetString(7);
                // Truncate VisibleText to 200 chars to reduce report size
                if (visibleText.Length > 200)
                {
                    visibleText = visibleText.Substring(0, 197) + "...";
                }

                var insight = new ActivityInsight
                {
                    TimestampUtc = processedAt.ToString("O"),
                    ActivityLabel = reader.IsDBNull(1) ? string.Empty : reader.GetString(1),
                    Application = application,
                    ContentType = contentType,
                    Topic = reader.IsDBNull(4) ? string.Empty : reader.GetString(4),
                    Action = reader.IsDBNull(5) ? string.Empty : reader.GetString(5),
                    Summary = reader.IsDBNull(6) ? string.Empty : reader.GetString(6),
                    VisibleText = visibleText,
                    Confidence = reader.GetDouble(8)
                };

                insights.Add(insight);

                if (!insightsByApplication.TryGetValue(insight.Application, out var list))
                {
                    list = new List<ActivityInsight>();
                    insightsByApplication[insight.Application] = list;
                }
                list.Add(insight);

                if (!contentSummaries.TryGetValue(insight.ContentType, out var summaryBuilder))
                {
                    summaryBuilder = new ContentTypeSummaryBuilder(insight.ContentType);
                    contentSummaries[insight.ContentType] = summaryBuilder;
                }
                summaryBuilder.Add(insight);
            }
        }

        // Build index map for insight deduplication
        var insightIndexByApplication = new Dictionary<string, List<int>>(StringComparer.OrdinalIgnoreCase);
        for (int i = 0; i < insights.Count; i++)
        {
            var app = insights[i].Application;
            if (!insightIndexByApplication.TryGetValue(app, out var indexList))
            {
                indexList = new List<int>();
                insightIndexByApplication[app] = indexList;
            }
            indexList.Add(i);
        }

        List<int> InsightIndexProvider(string processName) => MatchInsightIndices(processName, insightIndexByApplication);

        var report = new ActivityReport
        {
            GeneratedAtUtc = DateTime.UtcNow.ToString("O"),
            RangeStartUtc = fromUtc.ToString("O"),
            RangeEndUtc = toUtcExclusive.AddTicks(-1).ToString("O"),
            
            // Check-in/Check-out times
            CheckInTimeUtc = checkInTime?.ToString("O"),
            CheckOutTimeUtc = checkOutTime?.ToString("O"),
            
            TotalTrackedSeconds = durationSecondsTotal,
            TotalActiveSeconds = totalActiveSeconds,
            TotalIdleSeconds = totalIdleSeconds,
            FocusEventsAnalyzed = events.Count,
            DetailedActivities = insights,
            ContentTypeBreakdown = contentSummaries.Values
                .Select(builder => builder.ToSummary())
                .OrderByDescending(summary => summary.ActivityCount)
                .ToList(),
            Segments = segments
        };

        report.Applications = usageBuilders.Values
            .Select(builder => builder.ToReportEntry(InsightIndexProvider))
            .OrderByDescending(entry => entry.TotalActiveSeconds)
            .ToList();

        var jsonOptions = new JsonSerializerOptions
        {
            WriteIndented = true,
            DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull
        };

        var json = JsonSerializer.Serialize(report, jsonOptions);
        await File.WriteAllTextAsync(outputPath, json, Encoding.UTF8);

        return report;
    }

    // System processes to filter out from LLM reports (non-productive background processes)
    private static readonly HashSet<string> SystemProcessFilter = new(StringComparer.OrdinalIgnoreCase)
    {
        "SearchHost", "ShellHost", "ApplicationFrameHost", "StartMenuExperienceHost",
        "ShellExperienceHost", "dwm", "PickerHost", "TextInputHost", "SystemSettings",
        "LockApp", "SearchUI", "Taskmgr", "explorer", "RuntimeBroker", "svchost",
        "csrss", "winlogon", "fontdrvhost", "sihost", "ctfmon"
    };

    static async Task<LlmActivityReport> GenerateLlmReportAsync(string dbPath, DateTime fromLocal, DateTime toLocalInclusive, string outputPath)
    {
        var fromUtc = DateTime.SpecifyKind(fromLocal, DateTimeKind.Local).ToUniversalTime();
        var toUtcExclusive = DateTime.SpecifyKind(toLocalInclusive, DateTimeKind.Local).AddDays(1).ToUniversalTime();

        var connectionString = $"Data Source={dbPath}";

        using var connection = new SqliteConnection(connectionString);
        await connection.OpenAsync();

        // Load activity events (excluding system processes)
        var events = new List<EventSnapshot>();
        const string eventsSql = @"
            SELECT Timestamp, EventType, ProcessName, WindowTitle, IsIdle
            FROM ActivityEvents
            WHERE Timestamp BETWEEN @From AND @To
            ORDER BY Timestamp ASC
        ";

        using (var eventCommand = new SqliteCommand(eventsSql, connection))
        {
            eventCommand.Parameters.AddWithValue("@From", fromUtc.ToString("O"));
            eventCommand.Parameters.AddWithValue("@To", toUtcExclusive.ToString("O"));

            using var reader = await eventCommand.ExecuteReaderAsync();
            while (await reader.ReadAsync())
            {
                var processName = reader.IsDBNull(2) ? null : reader.GetString(2);
                
                // Filter out system processes
                if (!string.IsNullOrEmpty(processName) && SystemProcessFilter.Contains(processName))
                    continue;

                events.Add(new EventSnapshot
                {
                    TimestampUtc = ParseUtc(reader.GetString(0)),
                    EventType = reader.GetString(1),
                    ProcessName = processName,
                    WindowTitle = reader.IsDBNull(3) ? null : reader.GetString(3),
                    IsIdle = reader.GetInt32(4) == 1
                });
            }
        }

        // Load insights
        var insights = new List<ActivityInsight>();
        const string insightsSql = @"
            SELECT ProcessedAt, ActivityLabel, Application, ContentType, Topic, Action, Summary, VisibleText, Confidence
            FROM InferenceResults
            WHERE ProcessedAt BETWEEN @From AND @To
            ORDER BY ProcessedAt ASC
        ";

        using (var insightCommand = new SqliteCommand(insightsSql, connection))
        {
            insightCommand.Parameters.AddWithValue("@From", fromUtc.ToString("O"));
            insightCommand.Parameters.AddWithValue("@To", toUtcExclusive.ToString("O"));

            using var reader = await insightCommand.ExecuteReaderAsync();
            while (await reader.ReadAsync())
            {
                var application = reader.IsDBNull(2) ? "Unknown" : reader.GetString(2);
                
                // Filter out system process insights
                if (SystemProcessFilter.Contains(application))
                    continue;

                insights.Add(new ActivityInsight
                {
                    TimestampUtc = ParseUtc(reader.GetString(0)).ToString("O"),
                    ActivityLabel = reader.IsDBNull(1) ? string.Empty : reader.GetString(1),
                    Application = application,
                    ContentType = reader.IsDBNull(3) ? "Unknown" : reader.GetString(3),
                    Topic = reader.IsDBNull(4) ? string.Empty : reader.GetString(4),
                    Action = reader.IsDBNull(5) ? string.Empty : reader.GetString(5),
                    Summary = reader.IsDBNull(6) ? string.Empty : reader.GetString(6),
                    VisibleText = Truncate(reader.IsDBNull(7) ? string.Empty : reader.GetString(7), 150),
                    Confidence = reader.GetDouble(8)
                });
            }
        }

        // Calculate times
        DateTime? checkInTime = events.Count > 0 ? events[0].TimestampUtc : null;
        DateTime? checkOutTime = events.Count > 0 ? events[^1].TimestampUtc : null;
        var totalTrackedSeconds = (checkInTime.HasValue && checkOutTime.HasValue)
            ? (long)Math.Max(0, (checkOutTime.Value - checkInTime.Value).TotalSeconds)
            : 0L;

        // Infer projects from window titles and file paths
        var projects = InferProjects(events, insights);

        // Generate significant moments
        var significantMoments = ExtractSignificantMoments(events, insights);

        // Generate hourly breakdown
        var hourlyBreakdown = BuildHourlyBreakdown(insights);

        // Generate top activities (most informative insights)
        var topActivities = BuildTopActivities(insights);

        // Generate day patterns
        var patterns = BuildDayPatterns(events, insights);

        // Build executive summary using template-based approach
        var executiveSummary = BuildExecutiveSummary(fromLocal, toLocalInclusive, checkInTime, checkOutTime, 
            totalTrackedSeconds, insights, projects, events);

        // Build the LLM report
        var llmReport = new LlmActivityReport
        {
            GeneratedAtUtc = DateTime.UtcNow.ToString("O"),
            ReportDate = fromLocal.ToString("yyyy-MM-dd"),
            WorkingHours = new WorkingHoursInfo
            {
                Start = checkInTime?.ToLocalTime().ToString("HH:mm"),
                End = checkOutTime?.ToLocalTime().ToString("HH:mm"),
                TotalMinutes = (int)(totalTrackedSeconds / 60),
                TotalFormatted = FormatDuration(totalTrackedSeconds)
            },
            ExecutiveSummary = executiveSummary,
            HourlyBreakdown = hourlyBreakdown,
            Projects = projects,
            TopActivities = topActivities,
            SignificantMoments = significantMoments,
            Patterns = patterns,
            SchemaDescription = new LlmSchemaDescription
            {
                Purpose = "Daily activity report optimized for LLM consumption and analysis",
                ReadingOrder = new[] { "ExecutiveSummary", "HourlyBreakdown", "Projects", "TopActivities", "SignificantMoments", "Patterns" },
                KeyFields = new Dictionary<string, string>
                {
                    ["ExecutiveSummary"] = "Start here for high-level understanding of the day",
                    ["HourlyBreakdown"] = "Hour-by-hour view of what the user was working on",
                    ["Projects"] = "Activity grouped by inferred project or task context",
                    ["TopActivities"] = "Most significant individual activities with full context",
                    ["SignificantMoments"] = "Key events and transitions throughout the day",
                    ["Patterns"] = "Observed patterns, productivity insights, and frequent topics"
                }
            }
        };

        var jsonOptions = new JsonSerializerOptions
        {
            WriteIndented = true,
            DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull
        };

        var json = JsonSerializer.Serialize(llmReport, jsonOptions);
        await File.WriteAllTextAsync(outputPath, json, Encoding.UTF8);

        return llmReport;
    }

    static List<LlmHourlyBreakdown> BuildHourlyBreakdown(List<ActivityInsight> insights)
    {
        var hourly = new List<LlmHourlyBreakdown>();
        
        var groupedByHour = insights
            .GroupBy(i => ParseUtc(i.TimestampUtc).ToLocalTime().Hour)
            .OrderBy(g => g.Key);

        foreach (var hourGroup in groupedByHour)
        {
            var hour = hourGroup.Key;
            var hourInsights = hourGroup.ToList();
            
            var topContentType = hourInsights
                .Where(i => !string.IsNullOrEmpty(i.ContentType))
                .GroupBy(i => i.ContentType)
                .OrderByDescending(g => g.Count())
                .FirstOrDefault()?.Key ?? "mixed";

            var topApps = hourInsights
                .Where(i => !string.IsNullOrEmpty(i.Application))
                .GroupBy(i => i.Application)
                .OrderByDescending(g => g.Count())
                .Take(3)
                .Select(g => g.Key)
                .ToList();

            var topics = hourInsights
                .Where(i => !string.IsNullOrEmpty(i.Topic))
                .Select(i => i.Topic)
                .Distinct()
                .Take(5)
                .ToList();

            // Build summary from top insights
            var topInsight = hourInsights.OrderByDescending(i => i.Confidence).FirstOrDefault();
            var summary = topInsight != null 
                ? $"{topInsight.Action} {topInsight.Topic}".Trim()
                : $"{topContentType} activities";

            hourly.Add(new LlmHourlyBreakdown
            {
                Hour = $"{hour:D2}:00-{hour:D2}:59",
                PrimaryFocus = topContentType,
                Applications = topApps,
                Topics = topics,
                ActivityCount = hourInsights.Count,
                Summary = summary
            });
        }

        return hourly;
    }

    static List<LlmDetailedActivity> BuildTopActivities(List<ActivityInsight> insights)
    {
        // Select the most informative activities (high confidence, good summaries)
        return insights
            .Where(i => !string.IsNullOrEmpty(i.Summary) && i.Summary.Length > 20 && i.Confidence >= 0.6)
            .OrderByDescending(i => i.Confidence)
            .ThenByDescending(i => i.Summary.Length)
            .Take(20)
            .Select(i => new LlmDetailedActivity
            {
                Time = ParseUtc(i.TimestampUtc).ToLocalTime().ToString("HH:mm"),
                Application = i.Application,
                Activity = i.ActivityLabel,
                Topic = string.IsNullOrEmpty(i.Topic) ? null : i.Topic,
                Action = string.IsNullOrEmpty(i.Action) ? null : i.Action,
                Summary = Truncate(i.Summary, 250),
                ContentType = i.ContentType
            })
            .ToList();
    }

    static LlmDayPatterns BuildDayPatterns(List<EventSnapshot> events, List<ActivityInsight> insights)
    {
        var patterns = new LlmDayPatterns();

        // Most productive hour (most insights)
        var mostProductiveHour = insights
            .GroupBy(i => ParseUtc(i.TimestampUtc).ToLocalTime().Hour)
            .OrderByDescending(g => g.Count())
            .FirstOrDefault();
        
        if (mostProductiveHour != null)
        {
            patterns.MostProductiveHour = $"{mostProductiveHour.Key:D2}:00 ({mostProductiveHour.Count()} activities)";
        }

        // Count context switches (app changes)
        string? lastApp = null;
        int switches = 0;
        foreach (var evt in events.Where(e => !e.IsIdle))
        {
            if (lastApp != null && evt.ProcessName != lastApp)
            {
                switches++;
            }
            lastApp = evt.ProcessName;
        }
        patterns.ContextSwitches = switches;

        // Longest focus session
        string? focusApp = null;
        DateTime focusStart = DateTime.MinValue;
        TimeSpan longestFocus = TimeSpan.Zero;
        string? longestFocusApp = null;

        foreach (var evt in events.Where(e => !e.IsIdle))
        {
            if (focusApp == null || evt.ProcessName != focusApp)
            {
                if (focusApp != null)
                {
                    var duration = evt.TimestampUtc - focusStart;
                    if (duration > longestFocus)
                    {
                        longestFocus = duration;
                        longestFocusApp = focusApp;
                    }
                }
                focusApp = evt.ProcessName;
                focusStart = evt.TimestampUtc;
            }
        }

        if (longestFocus.TotalMinutes >= 5)
        {
            patterns.LongestFocusSession = $"{longestFocusApp} ({(int)longestFocus.TotalMinutes} minutes)";
        }

        // Frequent topics
        patterns.FrequentTopics = insights
            .Where(i => !string.IsNullOrEmpty(i.Topic))
            .GroupBy(i => i.Topic)
            .OrderByDescending(g => g.Count())
            .Take(8)
            .Select(g => $"{g.Key} ({g.Count()}x)")
            .ToList();

        // Observations
        var observations = new List<string>();
        
        var codePercentage = insights.Count > 0 
            ? (insights.Count(i => i.ContentType == "code") * 100 / insights.Count) 
            : 0;
        
        if (codePercentage > 50)
        {
            observations.Add($"Heavy coding day ({codePercentage}% of activities were code-related)");
        }
        else if (codePercentage > 30)
        {
            observations.Add($"Balanced day with significant coding ({codePercentage}%)");
        }

        if (switches > 50)
        {
            observations.Add($"High context switching ({switches} app changes) - may indicate multitasking or interruptions");
        }
        else if (switches < 20 && events.Count > 50)
        {
            observations.Add("Good focus with minimal context switching");
        }

        var webPercentage = insights.Count > 0 
            ? (insights.Count(i => i.ContentType == "web") * 100 / insights.Count) 
            : 0;
        
        if (webPercentage > 40)
        {
            observations.Add($"Significant web research/browsing ({webPercentage}%)");
        }

        patterns.Observations = observations.Count > 0 ? observations : null;

        return patterns;
    }

    static List<LlmProjectSummary> InferProjects(List<EventSnapshot> events, List<ActivityInsight> insights)
    {
        var projectData = new Dictionary<string, ProjectBuilder>(StringComparer.OrdinalIgnoreCase);

        // Project path patterns to extract project names
        var pathPatterns = new[]
        {
            @"[/\\]([^/\\]+)[/\\](src|lib|app|bin|obj|node_modules|\.git)[/\\]?",  // Common project structure
            @"[A-Za-z]:[/\\](?:Office|Projects?|Work|Dev|Code|repos?)[/\\]([^/\\]+)",  // Common dev folders
            @"[/\\]([^/\\]+)\s*[-–]\s*(Visual Studio|VS Code|Code|Antigravity)",  // Window title patterns
        };

        // Extract projects from window titles
        foreach (var evt in events.Where(e => !e.IsIdle && !string.IsNullOrEmpty(e.WindowTitle)))
        {
            var title = evt.WindowTitle!;
            string? projectName = null;

            foreach (var pattern in pathPatterns)
            {
                var match = Regex.Match(title, pattern, RegexOptions.IgnoreCase);
                if (match.Success && match.Groups.Count > 1)
                {
                    projectName = match.Groups[1].Value.Trim();
                    break;
                }
            }

            // Fallback: use process name + first meaningful part of window title
            if (string.IsNullOrEmpty(projectName) && !string.IsNullOrEmpty(evt.ProcessName))
            {
                var titleParts = title.Split(new[] { " - ", " – ", " | " }, StringSplitOptions.RemoveEmptyEntries);
                if (titleParts.Length > 0)
                {
                    var firstPart = titleParts[0].Trim();
                    if (firstPart.Length > 2 && firstPart.Length < 50)
                    {
                        projectName = $"{evt.ProcessName}: {Truncate(firstPart, 30)}";
                    }
                }
            }

            if (!string.IsNullOrEmpty(projectName))
            {
                if (!projectData.TryGetValue(projectName, out var builder))
                {
                    builder = new ProjectBuilder(projectName);
                    projectData[projectName] = builder;
                }
                builder.AddEvent(evt);
            }
        }

        // Enrich projects with insights
        foreach (var insight in insights)
        {
            // Try to match insight to a project
            foreach (var kvp in projectData)
            {
                if (insight.Application.Contains(kvp.Key, StringComparison.OrdinalIgnoreCase) ||
                    kvp.Key.Contains(insight.Application, StringComparison.OrdinalIgnoreCase) ||
                    (!string.IsNullOrEmpty(insight.Topic) && kvp.Key.Contains(insight.Topic, StringComparison.OrdinalIgnoreCase)))
                {
                    kvp.Value.AddInsight(insight);
                    break;
                }
            }
        }

        return projectData.Values
            .Where(p => p.EventCount > 0)
            .OrderByDescending(p => p.TotalSeconds)
            .Take(10)
            .Select(p => p.ToSummary())
            .ToList();
    }

    static List<LlmSignificantMoment> ExtractSignificantMoments(List<EventSnapshot> events, List<ActivityInsight> insights)
    {
        var moments = new List<LlmSignificantMoment>();

        if (events.Count == 0)
            return moments;

        // First activity (check-in)
        var first = events[0];
        moments.Add(new LlmSignificantMoment
        {
            Time = first.TimestampUtc.ToLocalTime().ToString("HH:mm"),
            Event = "Started work",
            Context = $"Began with {first.ProcessName ?? "unknown application"}",
            Application = first.ProcessName
        });

        // Track app switches (significant transitions)
        string? lastApp = first.ProcessName;
        DateTime lastSwitchTime = first.TimestampUtc;
        int consecutiveCount = 1;

        foreach (var evt in events.Skip(1))
        {
            if (evt.IsIdle) continue;

            if (evt.ProcessName != lastApp)
            {
                // Only record if we spent more than 5 minutes in the previous app
                var duration = evt.TimestampUtc - lastSwitchTime;
                if (duration.TotalMinutes >= 5 && consecutiveCount >= 3)
                {
                    // Find relevant insight for context
                    var relevantInsight = insights
                        .Where(i => Math.Abs((ParseUtc(i.TimestampUtc) - lastSwitchTime).TotalMinutes) < 10)
                        .OrderByDescending(i => i.Confidence)
                        .FirstOrDefault();

                    var context = relevantInsight != null && !string.IsNullOrEmpty(relevantInsight.Summary)
                        ? relevantInsight.Summary
                        : relevantInsight != null
                            ? $"{relevantInsight.Action} {relevantInsight.Topic}".Trim()
                            : $"Working in {lastApp}";

                    if (!string.IsNullOrWhiteSpace(context))
                    {
                        moments.Add(new LlmSignificantMoment
                        {
                            Time = lastSwitchTime.ToLocalTime().ToString("HH:mm"),
                            Event = $"Focus session: {lastApp}",
                            Context = Truncate(context, 120),
                            DurationMinutes = (int)duration.TotalMinutes,
                            Application = lastApp
                        });
                    }
                }

                lastApp = evt.ProcessName;
                lastSwitchTime = evt.TimestampUtc;
                consecutiveCount = 1;
            }
            else
            {
                consecutiveCount++;
            }
        }

        // Last activity (check-out)
        var last = events[^1];
        if (events.Count > 1)
        {
            // Calculate duration of last session
            var lastDuration = last.TimestampUtc - lastSwitchTime;
            
            moments.Add(new LlmSignificantMoment
            {
                Time = last.TimestampUtc.ToLocalTime().ToString("HH:mm"),
                Event = "Ended work",
                Context = $"Last activity in {last.ProcessName ?? "unknown application"}",
                DurationMinutes = lastDuration.TotalMinutes >= 1 ? (int)lastDuration.TotalMinutes : null,
                Application = last.ProcessName
            });
        }

        // Limit to 20 most significant moments
        return moments.Take(20).ToList();
    }

    static LlmExecutiveSummary BuildExecutiveSummary(DateTime fromLocal, DateTime toLocalInclusive, 
        DateTime? checkInTime, DateTime? checkOutTime, long totalTrackedSeconds,
        List<ActivityInsight> insights, List<LlmProjectSummary> projects, List<EventSnapshot> events)
    {
        // Calculate time breakdown
        var activeEvents = events.Where(e => !e.IsIdle).ToList();

        // Get unique applications
        var uniqueApps = activeEvents
            .Where(e => !string.IsNullOrEmpty(e.ProcessName))
            .Select(e => e.ProcessName!)
            .Distinct()
            .ToList();

        // Get unique topics
        var uniqueTopics = insights
            .Where(i => !string.IsNullOrEmpty(i.Topic))
            .Select(i => i.Topic)
            .Distinct()
            .ToList();

        // Get top applications by event count
        var topApps = activeEvents
            .Where(e => !string.IsNullOrEmpty(e.ProcessName))
            .GroupBy(e => e.ProcessName!)
            .OrderByDescending(g => g.Count())
            .Take(5)
            .Select(g => g.Key)
            .ToList();

        // Get content type breakdown
        var contentBreakdown = insights
            .Where(i => !string.IsNullOrEmpty(i.ContentType))
            .GroupBy(i => i.ContentType)
            .OrderByDescending(g => g.Count())
            .Take(5)
            .Select(g => new LlmCategoryBreakdown
            {
                Category = g.Key,
                Percentage = insights.Count > 0 ? (int)(g.Count() * 100.0 / insights.Count) : 0,
                Count = g.Count()
            })
            .ToList();

        // Extract technologies from topics and applications
        var techKeywords = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var insight in insights)
        {
            ExtractTechKeywords(insight.Topic, techKeywords);
            ExtractTechKeywords(insight.Application, techKeywords);
            ExtractTechKeywords(insight.Summary, techKeywords);
        }

        // Build narrative
        var narrative = BuildNarrative(fromLocal, checkInTime, checkOutTime, totalTrackedSeconds, 
            projects, topApps, insights);

        // Build accomplishments
        var accomplishments = BuildAccomplishments(insights, projects);

        // Build one-liner
        var topProject = projects.FirstOrDefault();
        var oneLiner = topProject != null
            ? $"Focused on {topProject.Name} ({topProject.TimeFormatted}) with {insights.Count} analyzed activities across {uniqueApps.Count} applications"
            : $"Worked for {FormatDuration(totalTrackedSeconds)} with {insights.Count} analyzed activities across {uniqueApps.Count} applications";

        return new LlmExecutiveSummary
        {
            OneLiner = oneLiner,
            Narrative = narrative,
            Accomplishments = accomplishments,
            TotalInsights = insights.Count,
            UniqueApplications = uniqueApps.Count,
            UniqueTopics = uniqueTopics.Count,
            PrimaryApplications = topApps,
            Technologies = techKeywords.Take(10).ToList(),
            MainWorkCategories = contentBreakdown
        };
    }

    static string? BuildAccomplishments(List<ActivityInsight> insights, List<LlmProjectSummary> projects)
    {
        var accomplishments = new List<string>();

        // Code-related accomplishments
        var codeInsights = insights.Where(i => i.ContentType == "code").ToList();
        if (codeInsights.Count > 0)
        {
            var filesEdited = codeInsights
                .Where(i => i.Action?.Contains("edit", StringComparison.OrdinalIgnoreCase) == true ||
                            i.Action?.Contains("writ", StringComparison.OrdinalIgnoreCase) == true)
                .Count();
            if (filesEdited > 0)
            {
                accomplishments.Add($"Edited/wrote code in {filesEdited} sessions");
            }
        }

        // Research accomplishments
        var researchInsights = insights.Where(i => 
            i.ContentType == "web" || 
            i.Action?.Contains("search", StringComparison.OrdinalIgnoreCase) == true ||
            i.Action?.Contains("read", StringComparison.OrdinalIgnoreCase) == true).ToList();
        if (researchInsights.Count > 5)
        {
            var topics = researchInsights
                .Where(i => !string.IsNullOrEmpty(i.Topic))
                .Select(i => i.Topic)
                .Distinct()
                .Take(3)
                .ToList();
            if (topics.Count > 0)
            {
                accomplishments.Add($"Researched: {string.Join(", ", topics)}");
            }
        }

        // Project accomplishments
        foreach (var project in projects.Take(2))
        {
            if (project.TimeMinutes > 30 && project.Activities?.Count > 0)
            {
                accomplishments.Add($"Worked on {project.Name}: {string.Join(", ", project.Activities.Take(3))}");
            }
        }

        return accomplishments.Count > 0 ? string.Join(". ", accomplishments) + "." : null;
    }

    static string BuildNarrative(DateTime fromLocal, DateTime? checkInTime, DateTime? checkOutTime,
        long totalTrackedSeconds, List<LlmProjectSummary> projects, List<string> topApps, List<ActivityInsight> insights)
    {
        var sb = new StringBuilder();

        // Opening
        if (checkInTime.HasValue && checkOutTime.HasValue)
        {
            var checkInLocal = checkInTime.Value.ToLocalTime();
            var checkOutLocal = checkOutTime.Value.ToLocalTime();
            sb.Append($"On {fromLocal:MMMM d, yyyy}, work started at {checkInLocal:h:mm tt} and ended at {checkOutLocal:h:mm tt} ");
            sb.Append($"({FormatDuration(totalTrackedSeconds)} tracked across {insights.Count} analyzed activities). ");
        }

        // Main activities with more detail
        if (projects.Count > 0)
        {
            var topProject = projects[0];
            sb.Append($"The primary focus was {topProject.Name}");
            if (!string.IsNullOrEmpty(topProject.Type))
            {
                sb.Append($" ({topProject.Type})");
            }
            if (topProject.TimeMinutes > 0)
            {
                sb.Append($", accounting for approximately {topProject.TimeFormatted}");
            }
            if (topProject.InsightCount > 0)
            {
                sb.Append($" with {topProject.InsightCount} recorded activities");
            }
            sb.Append(". ");

            // Add detail about what was done in top project
            if (topProject.TopicsWorkedOn?.Count > 0)
            {
                sb.Append($"Topics covered: {string.Join(", ", topProject.TopicsWorkedOn.Take(4))}. ");
            }

            if (projects.Count > 1)
            {
                sb.Append("Additional work included: ");
                var otherDetails = projects.Skip(1).Take(3)
                    .Select(p => $"{p.Name} ({p.TimeFormatted})")
                    .ToList();
                sb.Append(string.Join(", ", otherDetails));
                sb.Append(". ");
            }
        }

        // Content type breakdown
        var contentBreakdown = insights
            .Where(i => !string.IsNullOrEmpty(i.ContentType))
            .GroupBy(i => i.ContentType)
            .OrderByDescending(g => g.Count())
            .Take(3)
            .Select(g => $"{g.Key} ({g.Count()})")
            .ToList();

        if (contentBreakdown.Count > 0)
        {
            sb.Append($"Activity breakdown by type: {string.Join(", ", contentBreakdown)}. ");
        }

        // Key unique activities
        var uniqueActivities = insights
            .Where(i => !string.IsNullOrEmpty(i.ActivityLabel) && i.Confidence >= 0.6)
            .Select(i => i.ActivityLabel)
            .Distinct()
            .Take(6)
            .ToList();

        if (uniqueActivities.Count > 0)
        {
            sb.Append($"Notable activities: {string.Join(", ", uniqueActivities)}.");
        }

        return sb.ToString().Trim();
    }

    static void ExtractTechKeywords(string? text, HashSet<string> keywords)
    {
        if (string.IsNullOrEmpty(text)) return;

        var techTerms = new[]
        {
            "Python", "C#", "JavaScript", "TypeScript", "Java", "Go", "Rust", "Ruby", "PHP",
            "React", "Angular", "Vue", "Node", "Django", "Flask", "ASP.NET", ".NET",
            "Docker", "Kubernetes", "AWS", "Azure", "GCP", "PostgreSQL", "MySQL", "MongoDB",
            "Redis", "GraphQL", "REST", "API", "Git", "GitHub", "VS Code", "Visual Studio",
            "LangChain", "LangGraph", "LangSmith", "OpenAI", "Ollama", "Supabase", "Firebase"
        };

        foreach (var term in techTerms)
        {
            if (text.Contains(term, StringComparison.OrdinalIgnoreCase))
            {
                keywords.Add(term);
            }
        }
    }

    static async Task ShowDetailedActivityAsync(string? fromDate, string? toDate, int limit)
    {
        var dbPath = GetDatabasePath();
        if (dbPath == null) return;

        AnsiConsole.MarkupLine($"[green]Using database: {EscapeMarkup(dbPath)}[/]");
        AnsiConsole.WriteLine();

        // Convert local dates to UTC for database query. Treat the "to" date as inclusive by advancing one day.
        var fromLocal = string.IsNullOrEmpty(fromDate) ? DateTime.Today : DateTime.Parse(fromDate);
        var toLocalInclusive = string.IsNullOrEmpty(toDate) ? DateTime.Today : DateTime.Parse(toDate);
        var toLocal = toLocalInclusive.AddDays(1);

        var from = fromLocal.ToUniversalTime();
        var to = toLocal.ToUniversalTime();

        var connectionString = $"Data Source={dbPath}";

        using var connection = new SqliteConnection(connectionString);
        await connection.OpenAsync();

        var sql = @"
            SELECT 
                ProcessedAt,
                ActivityLabel,
                Application,
                ContentType,
                Topic,
                Action,
                Summary,
                VisibleText,
                Confidence
            FROM InferenceResults
            WHERE ProcessedAt BETWEEN @From AND @To
            ORDER BY ProcessedAt DESC
            LIMIT @Limit
        ";

        using var command = new SqliteCommand(sql, connection);
        command.Parameters.AddWithValue("@From", from.ToString("O"));
        command.Parameters.AddWithValue("@To", to.ToString("O"));
        command.Parameters.AddWithValue("@Limit", limit);

        var displayTo = toLocalInclusive;
        AnsiConsole.Write(new Rule($"[yellow]Detailed Activity Analysis: {fromLocal:yyyy-MM-dd} to {displayTo:yyyy-MM-dd}[/]"));
        AnsiConsole.WriteLine();

        using var reader = await command.ExecuteReaderAsync();
        int count = 0;

        while (await reader.ReadAsync())
        {
            count++;
            var timestamp = DateTime.Parse(reader.GetString(0)).ToLocalTime();

            var activityLabel = reader.IsDBNull(1) ? "Unknown activity" : reader.GetString(1);
            var application = reader.IsDBNull(2) ? "N/A" : reader.GetString(2);
            var contentType = reader.IsDBNull(3) ? "N/A" : reader.GetString(3);
            var topic = reader.IsDBNull(4) ? "N/A" : reader.GetString(4);
            var action = reader.IsDBNull(5) ? "N/A" : reader.GetString(5);
            var summary = reader.IsDBNull(6) ? "N/A" : reader.GetString(6);
            var visibleText = reader.IsDBNull(7) ? "N/A" : reader.GetString(7);
            var confidence = reader.GetDouble(8);

            var panelContent =
                $"[cyan bold]{EscapeMarkup(activityLabel)}[/]\n\n" +
                $"[dim]Application:[/] {EscapeMarkup(application)}\n" +
                $"[dim]Content Type:[/] {EscapeMarkup(contentType)}\n" +
                $"[dim]Topic:[/] {EscapeMarkup(topic)}\n" +
                $"[dim]Action:[/] {EscapeMarkup(action)}\n\n" +
                $"[yellow]Summary:[/]\n{EscapeMarkup(summary)}\n\n" +
                $"[dim]Visible Text:[/]\n[grey]{EscapeMarkup(visibleText)}[/]\n\n" +
                $"[dim]Confidence: {confidence:P0}[/]";

            var panel = new Panel(new Markup(panelContent));
            
            panel.Header = new PanelHeader($"[white]{timestamp:yyyy-MM-dd HH:mm:ss}[/]");
            panel.Border = BoxBorder.Rounded;
            
            AnsiConsole.Write(panel);
            AnsiConsole.WriteLine();
        }

        if (count == 0)
        {
            AnsiConsole.MarkupLine("[yellow]No detailed activity data found for this period.[/]");
            AnsiConsole.MarkupLine("[dim]Make sure Ollama is running and the service has been capturing screens.[/]");
        }
        else
        {
            AnsiConsole.MarkupLine($"[green]Showing {count} detailed activities[/]");
        }
    }

    static async Task ShowDailySummaryAsync(string? date)
    {
        var dbPath = GetDatabasePath();
        if (dbPath == null) return;

        AnsiConsole.MarkupLine($"[green]Using database: {EscapeMarkup(dbPath)}[/]");
        AnsiConsole.WriteLine();

        var targetDate = string.IsNullOrEmpty(date) ? DateTime.Today : DateTime.Parse(date);
        var from = targetDate.ToUniversalTime();
        var to = targetDate.AddDays(1).ToUniversalTime();

        var connectionString = $"Data Source={dbPath}";

        using var connection = new SqliteConnection(connectionString);
        await connection.OpenAsync();

        AnsiConsole.Write(new Rule($"[yellow]Daily Summary: {targetDate:yyyy-MM-dd}[/]"));
        AnsiConsole.WriteLine();

        // Activity by category
        var categorySql = @"
            SELECT 
                COALESCE(ContentType, 'Unknown') as Category,
                COUNT(*) as Count,
                GROUP_CONCAT(Topic, ', ') as Topics
            FROM InferenceResults
            WHERE ProcessedAt BETWEEN @From AND @To
            GROUP BY ContentType
            ORDER BY Count DESC
        ";

        using var catCommand = new SqliteCommand(categorySql, connection);
        catCommand.Parameters.AddWithValue("@From", from.ToString("O"));
        catCommand.Parameters.AddWithValue("@To", to.ToString("O"));

        var categoryTable = new Table();
        categoryTable.Title = new TableTitle("[cyan]Activity by Content Type[/]");
        categoryTable.AddColumn("Category");
        categoryTable.AddColumn("Count");
        categoryTable.AddColumn("Topics Covered");

        using (var reader = await catCommand.ExecuteReaderAsync())
        {
            while (await reader.ReadAsync())
            {
                var topics = reader.IsDBNull(2) ? "" : reader.GetString(2);
                if (topics.Length > 60)
                    topics = topics.Substring(0, 57) + "...";

                categoryTable.AddRow(
                    EscapeMarkup(reader.GetString(0)),
                    EscapeMarkup(reader.GetInt64(1).ToString()),
                    EscapeMarkup(topics)
                );
            }
        }

        AnsiConsole.Write(categoryTable);
        AnsiConsole.WriteLine();

        // Top applications
        var appSql = @"
            SELECT 
                COALESCE(Application, 'Unknown') as App,
                COUNT(*) as Count
            FROM InferenceResults
            WHERE ProcessedAt BETWEEN @From AND @To
            GROUP BY Application
            ORDER BY Count DESC
            LIMIT 10
        ";

        using var appCommand = new SqliteCommand(appSql, connection);
        appCommand.Parameters.AddWithValue("@From", from.ToString("O"));
        appCommand.Parameters.AddWithValue("@To", to.ToString("O"));

        var appTable = new Table();
        appTable.Title = new TableTitle("[cyan]Top Applications by Inference Count[/]");
        appTable.AddColumn("Application");
        appTable.AddColumn("Inference Count");

        using (var reader = await appCommand.ExecuteReaderAsync())
        {
            while (await reader.ReadAsync())
            {
                appTable.AddRow(
                    EscapeMarkup(reader.GetString(0)),
                    EscapeMarkup(reader.GetInt64(1).ToString())
                );
            }
        }

        AnsiConsole.Write(appTable);
        AnsiConsole.WriteLine();

        // Time distribution
        var timeSql = @"
            SELECT 
                COUNT(*) as TotalActivities,
                SUM(CASE WHEN ContentType = 'code' THEN 1 ELSE 0 END) as Coding,
                SUM(CASE WHEN ContentType LIKE '%web%' OR ContentType = 'article' THEN 1 ELSE 0 END) as WebBrowsing,
                SUM(CASE WHEN ContentType = 'document' THEN 1 ELSE 0 END) as Documents,
                SUM(CASE WHEN ContentType = 'video' THEN 1 ELSE 0 END) as Videos,
                SUM(CASE WHEN ContentType IN ('email', 'chat') THEN 1 ELSE 0 END) as Communication
            FROM InferenceResults
            WHERE ProcessedAt BETWEEN @From AND @To
        ";

        using var timeCommand = new SqliteCommand(timeSql, connection);
        timeCommand.Parameters.AddWithValue("@From", from.ToString("O"));
        timeCommand.Parameters.AddWithValue("@To", to.ToString("O"));

        using (var reader = await timeCommand.ExecuteReaderAsync())
        {
            if (await reader.ReadAsync())
            {
                var total = reader.GetInt64(0);
                
                if (total > 0)
                {
                    var chart = new BarChart()
                        .Width(60)
                        .Label("[green bold]Activity Distribution by Inference Count[/]");

                    chart.AddItem("Coding", reader.GetInt64(1), Color.Blue);
                    chart.AddItem("Web Browsing", reader.GetInt64(2), Color.Green);
                    chart.AddItem("Documents", reader.GetInt64(3), Color.Yellow);
                    chart.AddItem("Videos", reader.GetInt64(4), Color.Red);
                    chart.AddItem("Communication", reader.GetInt64(5), Color.Purple);

                    AnsiConsole.Write(chart);
                }
            }
        }
    }

    static string EscapeMarkup(string? text)
    {
        return Markup.Escape(text ?? string.Empty);
    }

    static DateTime ParseUtc(string value)
    {
        return DateTime.Parse(value, null, DateTimeStyles.AdjustToUniversal | DateTimeStyles.AssumeUniversal);
    }

    static string FormatDuration(long seconds)
    {
        if (seconds < 0)
        {
            seconds = 0;
        }

        var span = TimeSpan.FromSeconds(seconds);
        if (span.TotalHours >= 24)
        {
            var days = (int)span.TotalDays;
            var remainder = span - TimeSpan.FromDays(days);
            return $"{days}d {remainder:hh\\:mm\\:ss}";
        }

        if (span.TotalHours >= 1)
        {
            return span.ToString(@"hh\:mm\:ss");
        }

        return span.ToString(@"mm\:ss");
    }

    static List<ActivityInsight> MatchInsights(string processName, Dictionary<string, List<ActivityInsight>> insightsByApplication)
    {
        var results = new List<ActivityInsight>();

        if (string.IsNullOrWhiteSpace(processName))
        {
            return results;
        }

        var normalizedProcess = NormalizeKey(processName);
        var seen = new HashSet<string>();

        foreach (var kvp in insightsByApplication)
        {
            if (string.IsNullOrWhiteSpace(kvp.Key))
            {
                continue;
            }

            var normalizedApplication = NormalizeKey(kvp.Key);
            if (normalizedApplication.Length == 0)
            {
                continue;
            }

            if (normalizedApplication == normalizedProcess ||
                normalizedApplication.Contains(normalizedProcess, StringComparison.OrdinalIgnoreCase) ||
                normalizedProcess.Contains(normalizedApplication, StringComparison.OrdinalIgnoreCase))
            {
                foreach (var insight in kvp.Value)
                {
                    var token = $"{insight.TimestampUtc}|{insight.ActivityLabel}|{insight.Application}";
                    if (seen.Add(token))
                    {
                        results.Add(insight);
                    }
                }
            }
        }

        results.Sort((a, b) => string.CompareOrdinal(a.TimestampUtc, b.TimestampUtc));
        return results;
    }

    static List<int> MatchInsightIndices(string processName, Dictionary<string, List<int>> insightIndexByApplication)
    {
        var results = new List<int>();

        if (string.IsNullOrWhiteSpace(processName))
        {
            return results;
        }

        var normalizedProcess = NormalizeKey(processName);
        var seen = new HashSet<int>();

        foreach (var kvp in insightIndexByApplication)
        {
            if (string.IsNullOrWhiteSpace(kvp.Key))
            {
                continue;
            }

            var normalizedApplication = NormalizeKey(kvp.Key);
            if (normalizedApplication.Length == 0)
            {
                continue;
            }

            if (normalizedApplication == normalizedProcess ||
                normalizedApplication.Contains(normalizedProcess, StringComparison.OrdinalIgnoreCase) ||
                normalizedProcess.Contains(normalizedApplication, StringComparison.OrdinalIgnoreCase))
            {
                foreach (var index in kvp.Value)
                {
                    if (seen.Add(index))
                    {
                        results.Add(index);
                    }
                }
            }
        }

        results.Sort();
        return results;
    }

    static string Truncate(string? text, int maxLength, string suffix = "...")
    {
        if (string.IsNullOrEmpty(text) || text.Length <= maxLength)
        {
            return text ?? string.Empty;
        }
        
        return text.Substring(0, maxLength - suffix.Length) + suffix;
    }

    static string NormalizeKey(string? text)
    {
        if (string.IsNullOrWhiteSpace(text))
        {
            return string.Empty;
        }

        Span<char> buffer = stackalloc char[text.Length];
        var index = 0;

        foreach (var ch in text)
        {
            if (char.IsLetterOrDigit(ch))
            {
                buffer[index++] = char.ToLowerInvariant(ch);
            }
        }

        return new string(buffer[..index]);
    }

    static string? GetDatabasePath()
    {
        // First try the service's relative path
        var dbPath = Path.Combine(Directory.GetCurrentDirectory(), "Data", "ActivityData.db");
        
        // If not found, try the ProgramData location
        if (!File.Exists(dbPath))
        {
            dbPath = Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.CommonApplicationData),
                "ActivityMonitor",
                "Data",
                "ActivityData.db");
        }

        if (!File.Exists(dbPath))
        {
            AnsiConsole.MarkupLine("[red]Database not found![/]");
            AnsiConsole.MarkupLine("[yellow]Make sure the service is running first![/]");
            return null;
        }

        return dbPath;
    }

    class ActivityReport
    {
        public string SchemaVersion { get; set; } = "2.0";
        public string GeneratedAtUtc { get; set; } = string.Empty;
        public string RangeStartUtc { get; set; } = string.Empty;
        public string RangeEndUtc { get; set; } = string.Empty;
        
        // Check-in/Check-out times based on actual activity
        public string? CheckInTimeUtc { get; set; }
        public string? CheckOutTimeUtc { get; set; }
        
        public long TotalTrackedSeconds { get; set; }
        public long TotalActiveSeconds { get; set; }
        public long TotalIdleSeconds { get; set; }
        public int FocusEventsAnalyzed { get; set; }
        public List<ApplicationUsage> Applications { get; set; } = new();
        public List<ActivityInsight> DetailedActivities { get; set; } = new();
        public List<ContentTypeSummary> ContentTypeBreakdown { get; set; } = new();
        public List<TimelineSegment> Segments { get; set; } = new();
    }

    class ApplicationUsage
    {
        public string ProcessName { get; set; } = string.Empty;
        public long TotalActiveSeconds { get; set; }
        public List<WindowUsage> Windows { get; set; } = new();
        public List<int> InsightIndices { get; set; } = new();
    }

    class WindowUsage
    {
        public string Title { get; set; } = string.Empty;
        public long ActiveSeconds { get; set; }
    }

    class TimelineSegment
    {
        public string EndUtc { get; set; } = string.Empty;
        public long DurationSeconds { get; set; }
        public bool IsIdle { get; set; }
        public string ProcessName { get; set; } = string.Empty;
        public string? WindowTitle { get; set; }
    }

    class ActivityInsight
    {
        public string TimestampUtc { get; set; } = string.Empty;
        public string ActivityLabel { get; set; } = string.Empty;
        public string Application { get; set; } = string.Empty;
        public string ContentType { get; set; } = string.Empty;
        public string Topic { get; set; } = string.Empty;
        public string Action { get; set; } = string.Empty;
        public string Summary { get; set; } = string.Empty;
        public string VisibleText { get; set; } = string.Empty;
        public double Confidence { get; set; }
    }

    class ContentTypeSummary
    {
        public string ContentType { get; set; } = string.Empty;
        public int ActivityCount { get; set; }
        public List<string> DistinctTopics { get; set; } = new();
        public List<string> DistinctApplications { get; set; } = new();
    }

    class ContentTypeSummaryBuilder
    {
        private readonly HashSet<string> _topics = new(StringComparer.OrdinalIgnoreCase);
        private readonly HashSet<string> _applications = new(StringComparer.OrdinalIgnoreCase);
        private int _count;

        public ContentTypeSummaryBuilder(string contentType)
        {
            ContentType = string.IsNullOrWhiteSpace(contentType) ? "Unknown" : contentType;
        }

        public string ContentType { get; }

        public void Add(ActivityInsight insight)
        {
            _count++;

            if (!string.IsNullOrWhiteSpace(insight.Topic))
            {
                _topics.Add(insight.Topic);
            }

            if (!string.IsNullOrWhiteSpace(insight.Application))
            {
                _applications.Add(insight.Application);
            }
        }

        public ContentTypeSummary ToSummary()
        {
            return new ContentTypeSummary
            {
                ContentType = ContentType,
                ActivityCount = _count,
                DistinctTopics = _topics.OrderBy(t => t).ToList(),
                DistinctApplications = _applications.OrderBy(a => a).ToList()
            };
        }
    }

    class ApplicationUsageBuilder
    {
        private readonly Dictionary<string, long> _windowDurations = new(StringComparer.OrdinalIgnoreCase);
        private long _totalSeconds;

        public ApplicationUsageBuilder(string processName)
        {
            ProcessName = string.IsNullOrWhiteSpace(processName) ? "Unknown" : processName;
        }

        public string ProcessName { get; }

        public void AddDuration(long seconds, string? windowTitle)
        {
            if (seconds <= 0)
            {
                return;
            }

            _totalSeconds += seconds;

            var title = string.IsNullOrWhiteSpace(windowTitle) ? "Unknown" : windowTitle;
            if (_windowDurations.TryGetValue(title, out var current))
            {
                _windowDurations[title] = current + seconds;
            }
            else
            {
                _windowDurations[title] = seconds;
            }
        }

        public ApplicationUsage ToReportEntry(Func<string, List<int>> insightIndexProvider)
        {
            var windows = _windowDurations
                .OrderByDescending(kv => kv.Value)
                .Select(kv => new WindowUsage
                {
                    Title = kv.Key,
                    ActiveSeconds = kv.Value
                })
                .ToList();

            return new ApplicationUsage
            {
                ProcessName = ProcessName,
                TotalActiveSeconds = _totalSeconds,
                Windows = windows,
                InsightIndices = insightIndexProvider(ProcessName)
            };
        }
    }

    class EventSnapshot
    {
        public DateTime TimestampUtc { get; set; }
        public string EventType { get; set; } = string.Empty;
        public string? ProcessName { get; set; }
        public string? WindowTitle { get; set; }
        public bool IsIdle { get; set; }
    }

    class TimelineEntry
    {
        public DateTime Timestamp { get; set; }
        public string EventType { get; set; } = "";
        public string? ProcessName { get; set; }
        public string? WindowTitle { get; set; }
        public bool IsIdle { get; set; }
        public string? ActivityLabel { get; set; }
        public string? Application { get; set; }
        public string? ContentType { get; set; }
        public string? Topic { get; set; }
        public string? Action { get; set; }
        public string? Summary { get; set; }
        public string? VisibleText { get; set; }
    }

    // ==================== LLM Report Model Classes ====================

    class LlmActivityReport
    {
        public string SchemaVersion { get; set; } = "3.0-llm";
        public LlmSchemaDescription? SchemaDescription { get; set; }
        public string GeneratedAtUtc { get; set; } = string.Empty;
        public string ReportDate { get; set; } = string.Empty;
        public WorkingHoursInfo? WorkingHours { get; set; }
        public LlmExecutiveSummary? ExecutiveSummary { get; set; }
        public List<LlmHourlyBreakdown>? HourlyBreakdown { get; set; }
        public List<LlmProjectSummary>? Projects { get; set; }
        public List<LlmDetailedActivity>? TopActivities { get; set; }
        public List<LlmSignificantMoment>? SignificantMoments { get; set; }
        public LlmDayPatterns? Patterns { get; set; }
    }

    class LlmSchemaDescription
    {
        public string Purpose { get; set; } = string.Empty;
        public string[]? ReadingOrder { get; set; }
        public Dictionary<string, string>? KeyFields { get; set; }
    }

    class WorkingHoursInfo
    {
        public string? Start { get; set; }
        public string? End { get; set; }
        public int TotalMinutes { get; set; }
        public string? TotalFormatted { get; set; }
    }

    class LlmExecutiveSummary
    {
        public string OneLiner { get; set; } = string.Empty;
        public string Narrative { get; set; } = string.Empty;
        public string? Accomplishments { get; set; }
        public int TotalInsights { get; set; }
        public int UniqueApplications { get; set; }
        public int UniqueTopics { get; set; }
        public List<string>? PrimaryApplications { get; set; }
        public List<string>? Technologies { get; set; }
        public List<LlmCategoryBreakdown>? MainWorkCategories { get; set; }
    }

    class LlmCategoryBreakdown
    {
        public string Category { get; set; } = string.Empty;
        public int Percentage { get; set; }
        public int Count { get; set; }
    }

    class LlmHourlyBreakdown
    {
        public string Hour { get; set; } = string.Empty;
        public string? PrimaryFocus { get; set; }
        public List<string>? Applications { get; set; }
        public List<string>? Topics { get; set; }
        public int ActivityCount { get; set; }
        public string? Summary { get; set; }
    }

    class LlmProjectSummary
    {
        public string Name { get; set; } = string.Empty;
        public string? Type { get; set; }
        public string? Description { get; set; }
        public string? WorkSummary { get; set; }
        public int TimeMinutes { get; set; }
        public string? TimeFormatted { get; set; }
        public List<string>? KeyFiles { get; set; }
        public List<string>? TopicsWorkedOn { get; set; }
        public List<string>? Activities { get; set; }
        public int InsightCount { get; set; }
    }

    class LlmDetailedActivity
    {
        public string Time { get; set; } = string.Empty;
        public string Application { get; set; } = string.Empty;
        public string Activity { get; set; } = string.Empty;
        public string? Topic { get; set; }
        public string? Action { get; set; }
        public string Summary { get; set; } = string.Empty;
        public string ContentType { get; set; } = string.Empty;
    }

    class LlmSignificantMoment
    {
        public string Time { get; set; } = string.Empty;
        public string Event { get; set; } = string.Empty;
        public string? Context { get; set; }
        public int? DurationMinutes { get; set; }
        public string? Application { get; set; }
    }

    class LlmDayPatterns
    {
        public string? MostProductiveHour { get; set; }
        public string? LongestFocusSession { get; set; }
        public int ContextSwitches { get; set; }
        public List<string>? FrequentTopics { get; set; }
        public List<string>? Observations { get; set; }
    }

    // ==================== LLM Report Builder Helpers ====================

    class ProjectBuilder
    {
        private readonly string _name;
        private readonly List<EventSnapshot> _events = new();
        private readonly List<ActivityInsight> _insights = new();
        private readonly HashSet<string> _files = new(StringComparer.OrdinalIgnoreCase);
        private readonly HashSet<string> _activities = new(StringComparer.OrdinalIgnoreCase);
        private long _totalSeconds;

        public ProjectBuilder(string name)
        {
            _name = name;
        }

        public int EventCount => _events.Count;
        public long TotalSeconds => _totalSeconds;

        public void AddEvent(EventSnapshot evt)
        {
            _events.Add(evt);

            // Estimate time: assume each event represents ~30 seconds of activity
            _totalSeconds += 30;

            // Extract file names from window titles
            if (!string.IsNullOrEmpty(evt.WindowTitle))
            {
                var fileMatch = Regex.Match(
                    evt.WindowTitle, 
                    @"([a-zA-Z_][a-zA-Z0-9_]*\.(py|cs|js|ts|json|yaml|yml|md|txt|html|css|sql))",
                    RegexOptions.IgnoreCase);
                if (fileMatch.Success)
                {
                    _files.Add(fileMatch.Value);
                }
            }
        }

        public void AddInsight(ActivityInsight insight)
        {
            _insights.Add(insight);

            if (!string.IsNullOrEmpty(insight.ActivityLabel))
            {
                _activities.Add(insight.ActivityLabel);
            }

            if (!string.IsNullOrEmpty(insight.Action) && !string.IsNullOrEmpty(insight.Topic))
            {
                _activities.Add($"{insight.Action} {insight.Topic}".Trim());
            }
        }

        public LlmProjectSummary ToSummary()
        {
            // Infer project type from insights
            string? projectType = null;
            var contentTypes = _insights
                .Where(i => !string.IsNullOrEmpty(i.ContentType))
                .GroupBy(i => i.ContentType)
                .OrderByDescending(g => g.Count())
                .FirstOrDefault();
            
            if (contentTypes != null)
            {
                projectType = contentTypes.Key switch
                {
                    "code" => "Software Development",
                    "document" => "Documentation",
                    "web" => "Web Research",
                    "chat" => "Communication",
                    "video" => "Media",
                    "image" => "Visual/Design",
                    _ => contentTypes.Key
                };
            }

            // Build description from top insights (prefer high confidence)
            var topInsight = _insights
                .OrderByDescending(i => i.Confidence)
                .ThenByDescending(i => i.Summary?.Length ?? 0)
                .FirstOrDefault();
            var description = topInsight != null && !string.IsNullOrEmpty(topInsight.Summary)
                ? topInsight.Summary
                : $"Work related to {_name}";

            // Build work summary from multiple insights
            var workActions = _insights
                .Where(i => !string.IsNullOrEmpty(i.Action) && !string.IsNullOrEmpty(i.Topic))
                .Select(i => $"{i.Action} {i.Topic}".Trim())
                .Distinct()
                .Take(5)
                .ToList();
            var workSummary = workActions.Count > 0 ? string.Join("; ", workActions) : null;

            // Get topics worked on
            var topics = _insights
                .Where(i => !string.IsNullOrEmpty(i.Topic))
                .Select(i => i.Topic)
                .Distinct()
                .Take(8)
                .ToList();

            var timeMinutes = (int)(_totalSeconds / 60);

            return new LlmProjectSummary
            {
                Name = _name,
                Type = projectType,
                Description = description?.Length > 200 ? description.Substring(0, 197) + "..." : description,
                WorkSummary = workSummary,
                TimeMinutes = timeMinutes,
                TimeFormatted = FormatDurationStatic(timeMinutes * 60),
                KeyFiles = _files.Count > 0 ? _files.Take(10).ToList() : null,
                TopicsWorkedOn = topics.Count > 0 ? topics : null,
                Activities = _activities.Count > 0 ? _activities.Take(8).ToList() : null,
                InsightCount = _insights.Count
            };
        }

        private static string FormatDurationStatic(long seconds)
        {
            if (seconds < 0) seconds = 0;
            var span = TimeSpan.FromSeconds(seconds);
            if (span.TotalHours >= 1)
            {
                return $"{(int)span.TotalHours}h {span.Minutes}m";
            }
            return $"{span.Minutes}m";
        }
    }
}
