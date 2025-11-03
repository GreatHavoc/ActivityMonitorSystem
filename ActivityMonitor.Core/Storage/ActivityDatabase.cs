using ActivityMonitor.Common.Configuration;
using ActivityMonitor.Common.Models;
using Microsoft.Data.Sqlite;
using Microsoft.Extensions.Logging;
using Microsoft.Extensions.Options;
using System.Text.Json;

namespace ActivityMonitor.Core.Storage;

/// <summary>
/// SQLite database for storing activity events and inference results
/// </summary>
public class ActivityDatabase
{
    private readonly ILogger<ActivityDatabase> _logger;
    private readonly ActivityMonitorSettings _settings;
    private readonly string _connectionString;

    public ActivityDatabase(
        ILogger<ActivityDatabase> logger,
        IOptions<ActivityMonitorSettings> settings)
    {
        _logger = logger;
        _settings = settings.Value;
        
        var dbPath = Path.GetFullPath(_settings.Storage.DatabasePath);
        var dbDirectory = Path.GetDirectoryName(dbPath);
        
        if (!string.IsNullOrEmpty(dbDirectory) && !Directory.Exists(dbDirectory))
        {
            Directory.CreateDirectory(dbDirectory);
        }

        _connectionString = $"Data Source={dbPath}";
    }

    public async Task InitializeAsync(CancellationToken cancellationToken)
    {
        _logger.LogInformation("Initializing database at {DatabasePath}", _settings.Storage.DatabasePath);

        using var connection = new SqliteConnection(_connectionString);
        await connection.OpenAsync(cancellationToken);

        // Create activity events table
        var createEventsTable = @"
            CREATE TABLE IF NOT EXISTS ActivityEvents (
                Id INTEGER PRIMARY KEY AUTOINCREMENT,
                Timestamp TEXT NOT NULL,
                EventType TEXT NOT NULL,
                ProcessId INTEGER,
                ProcessName TEXT,
                WindowTitle TEXT,
                IsIdle INTEGER NOT NULL,
                IdleDurationSeconds INTEGER,
                Metadata TEXT
            );
            
            CREATE INDEX IF NOT EXISTS IX_ActivityEvents_Timestamp 
                ON ActivityEvents(Timestamp);
            CREATE INDEX IF NOT EXISTS IX_ActivityEvents_EventType 
                ON ActivityEvents(EventType);
            CREATE INDEX IF NOT EXISTS IX_ActivityEvents_ProcessName 
                ON ActivityEvents(ProcessName);
        ";

        // Create inference results table
        var createResultsTable = @"
            CREATE TABLE IF NOT EXISTS InferenceResults (
                Id INTEGER PRIMARY KEY AUTOINCREMENT,
                RequestId TEXT NOT NULL UNIQUE,
                ProcessedAt TEXT NOT NULL,
                ActivityLabel TEXT NOT NULL,
                Application TEXT,
                ContentType TEXT,
                Topic TEXT,
                Action TEXT,
                Summary TEXT NOT NULL,
                VisibleText TEXT,
                Confidence REAL NOT NULL,
                DetectedObjects TEXT,
                RawResponse TEXT
            );
            
            CREATE INDEX IF NOT EXISTS IX_InferenceResults_ProcessedAt 
                ON InferenceResults(ProcessedAt);
            CREATE INDEX IF NOT EXISTS IX_InferenceResults_ActivityLabel 
                ON InferenceResults(ActivityLabel);
        ";

        using var command = connection.CreateCommand();
        command.CommandText = createEventsTable + createResultsTable;
        await command.ExecuteNonQueryAsync(cancellationToken);

        // Auto-migration: Add new columns if they don't exist
        await MigrateSchemaAsync(connection, cancellationToken);

        _logger.LogInformation("Database initialized successfully");
    }

    private async Task MigrateSchemaAsync(SqliteConnection connection, CancellationToken cancellationToken)
    {
        try
        {
            // Check if new columns exist
            var checkColumnsSql = "PRAGMA table_info(InferenceResults)";
            var existingColumns = new HashSet<string>(StringComparer.OrdinalIgnoreCase);

            using (var checkCmd = new SqliteCommand(checkColumnsSql, connection))
            using (var reader = await checkCmd.ExecuteReaderAsync(cancellationToken))
            {
                while (await reader.ReadAsync(cancellationToken))
                {
                    existingColumns.Add(reader.GetString(1)); // Column name is at index 1
                }
            }

            // Add missing columns (exact names as used in CREATE TABLE)
            var columnsToAdd = new Dictionary<string, string>
            {
                { "Application", "TEXT" },
                { "ContentType", "TEXT" },
                { "Topic", "TEXT" },
                { "Action", "TEXT" },
                { "VisibleText", "TEXT" }
            };

            foreach (var column in columnsToAdd)
            {
                if (!existingColumns.Contains(column.Key))
                {
                    var alterSql = $"ALTER TABLE InferenceResults ADD COLUMN {column.Key} {column.Value}";
                    using var alterCmd = new SqliteCommand(alterSql, connection);
                    await alterCmd.ExecuteNonQueryAsync(cancellationToken);
                    _logger.LogInformation("Added column {ColumnName} to InferenceResults table", column.Key);
                }
            }
        }
        catch (Exception ex)
        {
            _logger.LogWarning(ex, "Error during schema migration, columns may already exist");
        }
    }

    public async Task SaveActivityEventAsync(ActivityEvent activityEvent, CancellationToken cancellationToken)
    {
        try
        {
            using var connection = new SqliteConnection(_connectionString);
            await connection.OpenAsync(cancellationToken);

            const string sql = @"
                INSERT INTO ActivityEvents 
                    (Timestamp, EventType, ProcessId, ProcessName, WindowTitle, IsIdle, IdleDurationSeconds, Metadata)
                VALUES 
                    (@Timestamp, @EventType, @ProcessId, @ProcessName, @WindowTitle, @IsIdle, @IdleDurationSeconds, @Metadata)
            ";

            using var command = new SqliteCommand(sql, connection);
            command.Parameters.AddWithValue("@Timestamp", activityEvent.Timestamp.ToString("O"));
            command.Parameters.AddWithValue("@EventType", activityEvent.EventType);
            command.Parameters.AddWithValue("@ProcessId", (object?)activityEvent.ProcessId ?? DBNull.Value);
            command.Parameters.AddWithValue("@ProcessName", (object?)activityEvent.ProcessName ?? DBNull.Value);
            command.Parameters.AddWithValue("@WindowTitle", (object?)activityEvent.WindowTitle ?? DBNull.Value);
            command.Parameters.AddWithValue("@IsIdle", activityEvent.IsIdle ? 1 : 0);
            command.Parameters.AddWithValue("@IdleDurationSeconds", (object?)activityEvent.IdleDurationSeconds ?? DBNull.Value);
            command.Parameters.AddWithValue("@Metadata", (object?)activityEvent.Metadata ?? DBNull.Value);

            await command.ExecuteNonQueryAsync(cancellationToken);
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Error saving activity event");
        }
    }

    public async Task SaveInferenceResultAsync(InferenceResult result, CancellationToken cancellationToken)
    {
        try
        {
            using var connection = new SqliteConnection(_connectionString);
            await connection.OpenAsync(cancellationToken);

            const string sql = @"
                INSERT INTO InferenceResults 
                    (RequestId, ProcessedAt, ActivityLabel, Application, ContentType, Topic, Action, Summary, VisibleText, Confidence, DetectedObjects, RawResponse)
                VALUES 
                    (@RequestId, @ProcessedAt, @ActivityLabel, @Application, @ContentType, @Topic, @Action, @Summary, @VisibleText, @Confidence, @DetectedObjects, @RawResponse)
            ";

            using var command = new SqliteCommand(sql, connection);
            command.Parameters.AddWithValue("@RequestId", result.RequestId.ToString());
            command.Parameters.AddWithValue("@ProcessedAt", result.ProcessedAt.ToString("O"));
            command.Parameters.AddWithValue("@ActivityLabel", result.ActivityLabel);
            command.Parameters.AddWithValue("@Application", (object?)result.Application ?? DBNull.Value);
            command.Parameters.AddWithValue("@ContentType", (object?)result.ContentType ?? DBNull.Value);
            command.Parameters.AddWithValue("@Topic", (object?)result.Topic ?? DBNull.Value);
            command.Parameters.AddWithValue("@Action", (object?)result.Action ?? DBNull.Value);
            command.Parameters.AddWithValue("@Summary", result.Summary);
            command.Parameters.AddWithValue("@VisibleText", (object?)result.VisibleText ?? DBNull.Value);
            command.Parameters.AddWithValue("@Confidence", result.Confidence);
            
            var detectedObjectsJson = result.DetectedObjects != null 
                ? JsonSerializer.Serialize(result.DetectedObjects) 
                : null;
            command.Parameters.AddWithValue("@DetectedObjects", (object?)detectedObjectsJson ?? DBNull.Value);
            command.Parameters.AddWithValue("@RawResponse", (object?)result.RawResponse ?? DBNull.Value);

            await command.ExecuteNonQueryAsync(cancellationToken);

            _logger.LogDebug("Saved inference result for request {RequestId}", result.RequestId);
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Error saving inference result");
        }
    }

    public async Task<List<ActivityEvent>> QueryEventsAsync(
        DateTime startTime, 
        DateTime endTime, 
        CancellationToken cancellationToken)
    {
        var events = new List<ActivityEvent>();

        try
        {
            using var connection = new SqliteConnection(_connectionString);
            await connection.OpenAsync(cancellationToken);

            const string sql = @"
                SELECT Id, Timestamp, EventType, ProcessId, ProcessName, WindowTitle, 
                       IsIdle, IdleDurationSeconds, Metadata
                FROM ActivityEvents
                WHERE Timestamp BETWEEN @StartTime AND @EndTime
                ORDER BY Timestamp DESC
            ";

            using var command = new SqliteCommand(sql, connection);
            command.Parameters.AddWithValue("@StartTime", startTime.ToString("O"));
            command.Parameters.AddWithValue("@EndTime", endTime.ToString("O"));

            using var reader = await command.ExecuteReaderAsync(cancellationToken);
            
            while (await reader.ReadAsync(cancellationToken))
            {
                events.Add(new ActivityEvent
                {
                    Id = reader.GetInt64(0),
                    Timestamp = DateTime.Parse(reader.GetString(1)),
                    EventType = reader.GetString(2),
                    ProcessId = reader.IsDBNull(3) ? null : reader.GetInt32(3),
                    ProcessName = reader.IsDBNull(4) ? null : reader.GetString(4),
                    WindowTitle = reader.IsDBNull(5) ? null : reader.GetString(5),
                    IsIdle = reader.GetInt32(6) == 1,
                    IdleDurationSeconds = reader.IsDBNull(7) ? null : reader.GetInt32(7),
                    Metadata = reader.IsDBNull(8) ? null : reader.GetString(8)
                });
            }
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Error querying activity events");
        }

        return events;
    }

    public async Task<int> CompactOldEventsAsync(CancellationToken cancellationToken)
    {
        try
        {
            var cutoffDate = DateTime.UtcNow.AddDays(-_settings.Storage.MaxEventAgeDays);

            using var connection = new SqliteConnection(_connectionString);
            await connection.OpenAsync(cancellationToken);

            const string sql = "DELETE FROM ActivityEvents WHERE Timestamp < @CutoffDate";

            using var command = new SqliteCommand(sql, connection);
            command.Parameters.AddWithValue("@CutoffDate", cutoffDate.ToString("O"));

            var deleted = await command.ExecuteNonQueryAsync(cancellationToken);

            if (deleted > 0)
            {
                _logger.LogInformation("Compacted {Count} old events", deleted);
            }

            // Vacuum database to reclaim space
            using var vacuumCommand = new SqliteCommand("VACUUM", connection);
            await vacuumCommand.ExecuteNonQueryAsync(cancellationToken);

            return deleted;
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Error compacting old events");
            return 0;
        }
    }
}
