using ActivityMonitor.Common.Configuration;
using ActivityMonitor.Common.Models;
using ActivityMonitor.Core.Capture;
using ActivityMonitor.Core.Inference;
using ActivityMonitor.Core.Storage;
using Microsoft.Extensions.Logging;
using Microsoft.Extensions.Options;
using System.Collections.Concurrent;
using System.Diagnostics;

namespace ActivityMonitor.Core.Queue;

/// <summary>
/// Manages a priority-based queue for processing inference requests
/// Handles backpressure, rate limiting, and concurrent processing
/// </summary>
public class RequestQueueManager
{
    private readonly ILogger<RequestQueueManager> _logger;
    private readonly ActivityMonitorSettings _settings;
    private readonly CaptureManager _captureManager;
    private readonly OllamaInferenceClient _inferenceClient;
    private readonly ActivityDatabase _database;
    
    private readonly PriorityBlockingCollection<InferenceRequest> _queue;
    private readonly ConcurrentDictionary<Guid, Task> _activeTasks;
    private readonly SemaphoreSlim _concurrencySemaphore;
    
    private CancellationTokenSource? _processingCts;
    private Task? _processingTask;
    
    // Metrics
    private long _totalProcessed = 0;
    private long _totalFailed = 0;
    private readonly ConcurrentQueue<TimeSpan> _processingTimes;
    private const int MaxProcessingTimeSamples = 100;

    public RequestQueueManager(
        ILogger<RequestQueueManager> logger,
        IOptions<ActivityMonitorSettings> settings,
        CaptureManager captureManager,
        OllamaInferenceClient inferenceClient,
        ActivityDatabase database)
    {
        _logger = logger;
        _settings = settings.Value;
        _captureManager = captureManager;
        _inferenceClient = inferenceClient;
        _database = database;
        
        _queue = new PriorityBlockingCollection<InferenceRequest>(_settings.QueueSettings.MaxQueueSize);
        _activeTasks = new ConcurrentDictionary<Guid, Task>();
        _concurrencySemaphore = new SemaphoreSlim(_settings.QueueSettings.MaxConcurrentTasks);
        _processingTimes = new ConcurrentQueue<TimeSpan>();
    }

    public async Task StartAsync(CancellationToken cancellationToken)
    {
        _logger.LogInformation("Starting Request Queue Manager with {MaxConcurrent} workers", 
            _settings.QueueSettings.MaxConcurrentTasks);

        _processingCts = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        _processingTask = Task.Run(() => ProcessQueueAsync(_processingCts.Token), cancellationToken);

        await Task.CompletedTask;
    }

    public async Task StopAsync()
    {
        _logger.LogInformation("Stopping Request Queue Manager");

        _processingCts?.Cancel();

        if (_processingTask != null)
        {
            try
            {
                await _processingTask;
            }
            catch (OperationCanceledException)
            {
                // Expected during shutdown
            }
        }

        // Wait for active tasks to complete
        await Task.WhenAll(_activeTasks.Values);

        _logger.LogInformation("Request Queue Manager stopped. Processed: {Processed}, Failed: {Failed}", 
            _totalProcessed, _totalFailed);
    }

    public Task<bool> EnqueueAsync(InferenceRequest request, CancellationToken cancellationToken)
    {
        try
        {
            var added = _queue.TryAdd(request, request.Priority);
            
            if (added)
            {
                _logger.LogDebug("Enqueued request {RequestId} with priority {Priority}", 
                    request.Id, request.Priority);
                return Task.FromResult(true);
            }
            else
            {
                _logger.LogWarning("Queue is full, dropping request {RequestId}", request.Id);
                return Task.FromResult(false);
            }
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Error enqueueing request {RequestId}", request.Id);
            return Task.FromResult(false);
        }
    }

    private async Task ProcessQueueAsync(CancellationToken cancellationToken)
    {
        _logger.LogInformation("Queue processing started");

        while (!cancellationToken.IsCancellationRequested)
        {
            try
            {
                // Wait for queue item or cancellation
                var request = await _queue.TakeAsync(cancellationToken);
                
                // Wait for available processing slot
                await _concurrencySemaphore.WaitAsync(cancellationToken);

                // Process request asynchronously
                var task = Task.Run(async () =>
                {
                    try
                    {
                        await ProcessRequestAsync(request, cancellationToken);
                    }
                    finally
                    {
                        _concurrencySemaphore.Release();
                        _activeTasks.TryRemove(request.Id, out _);
                    }
                }, cancellationToken);

                _activeTasks[request.Id] = task;
            }
            catch (OperationCanceledException)
            {
                // Expected during shutdown
                break;
            }
            catch (Exception ex)
            {
                _logger.LogError(ex, "Error in queue processing loop");
                await Task.Delay(TimeSpan.FromSeconds(1), cancellationToken);
            }
        }

        _logger.LogInformation("Queue processing stopped");
    }

    private async Task ProcessRequestAsync(InferenceRequest request, CancellationToken cancellationToken)
    {
        var stopwatch = Stopwatch.StartNew();

        try
        {
            _logger.LogInformation("Processing request {RequestId} triggered by {Reason}", 
                request.Id, request.TriggerReason);

            // Step 1: Capture screen frames
            var frames = await _captureManager.CaptureFramesAsync(cancellationToken);
            
            if (frames == null || frames.Count == 0)
            {
                _logger.LogWarning("No frames captured for request {RequestId}", request.Id);
                Interlocked.Increment(ref _totalFailed);
                return;
            }

            request.CapturedFrames = frames;
            _logger.LogDebug("Captured {FrameCount} frames for request {RequestId}", 
                frames.Count, request.Id);

            // Step 2: Send to inference (no timeout - wait as long as needed)
            var result = await _inferenceClient.AnalyzeFramesAsync(frames, cancellationToken);

            if (result != null)
            {
                result.RequestId = request.Id;
                
                // Step 3: Save results to database
                await _database.SaveInferenceResultAsync(result, cancellationToken);

                _logger.LogInformation(
                    "Successfully processed request {RequestId}. Activity: {Activity}, Confidence: {Confidence:P0}", 
                    request.Id, result.ActivityLabel, result.Confidence);

                Interlocked.Increment(ref _totalProcessed);
            }
            else
            {
                _logger.LogWarning("Inference returned null result for request {RequestId}", request.Id);
                Interlocked.Increment(ref _totalFailed);
            }

            stopwatch.Stop();
            RecordProcessingTime(stopwatch.Elapsed);
        }
        catch (OperationCanceledException)
        {
            _logger.LogWarning("Request {RequestId} was cancelled or timed out", request.Id);
            Interlocked.Increment(ref _totalFailed);
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Error processing request {RequestId}", request.Id);
            Interlocked.Increment(ref _totalFailed);
        }
    }

    private void RecordProcessingTime(TimeSpan duration)
    {
        _processingTimes.Enqueue(duration);
        
        // Keep only recent samples
        while (_processingTimes.Count > MaxProcessingTimeSamples)
        {
            _processingTimes.TryDequeue(out _);
        }
    }

    public QueueMetrics GetMetrics()
    {
        var avgProcessingTime = _processingTimes.Any() 
            ? TimeSpan.FromMilliseconds(_processingTimes.Average(t => t.TotalMilliseconds))
            : TimeSpan.Zero;

        return new QueueMetrics
        {
            QueuedRequests = _queue.Count,
            ProcessingRequests = _activeTasks.Count,
            CompletedRequests = (int)_totalProcessed,
            FailedRequests = (int)_totalFailed,
            AverageProcessingTime = avgProcessingTime,
            LastUpdated = DateTime.UtcNow
        };
    }
}
