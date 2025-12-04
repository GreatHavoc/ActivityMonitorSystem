using Microsoft.Data.Sqlite;
using Spectre.Console;
using System.Globalization;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
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
        AnsiConsole.MarkupLine("               Options: --from <date> --to <date> --output <path>");
        AnsiConsole.MarkupLine("               Example: ActivityMonitor.CLI report --from \"2025-10-22\" --output report.json");
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
}
