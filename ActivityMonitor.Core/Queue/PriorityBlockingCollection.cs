using ActivityMonitor.Common.Models;
using System.Collections.Concurrent;
using System.Threading.Channels;

namespace ActivityMonitor.Core.Queue;

/// <summary>
/// Thread-safe priority-based blocking collection
/// Higher priority items are dequeued first
/// </summary>
public class PriorityBlockingCollection<T> where T : class
{
    private readonly Channel<T>[] _priorityChannels;
    private readonly SemaphoreSlim _itemAvailable;
    private readonly int _maxCapacity;
    private int _currentCount;

    public PriorityBlockingCollection(int maxCapacity)
    {
        _maxCapacity = maxCapacity;
        _itemAvailable = new SemaphoreSlim(0);
        
        // Create a channel for each priority level
        var priorityCount = Enum.GetValues<RequestPriority>().Length;
        _priorityChannels = new Channel<T>[priorityCount];
        
        for (int i = 0; i < priorityCount; i++)
        {
            _priorityChannels[i] = Channel.CreateUnbounded<T>(new UnboundedChannelOptions
            {
                SingleReader = false,
                SingleWriter = false
            });
        }
    }

    public int Count => _currentCount;

    public bool TryAdd(T item, RequestPriority priority)
    {
        // Check capacity
        if (Interlocked.Increment(ref _currentCount) > _maxCapacity)
        {
            Interlocked.Decrement(ref _currentCount);
            return false;
        }

        var channelIndex = (int)priority;
        var channel = _priorityChannels[channelIndex];

        if (channel.Writer.TryWrite(item))
        {
            _itemAvailable.Release();
            return true;
        }

        Interlocked.Decrement(ref _currentCount);
        return false;
    }

    public async Task<T> TakeAsync(CancellationToken cancellationToken)
    {
        while (!cancellationToken.IsCancellationRequested)
        {
            await _itemAvailable.WaitAsync(cancellationToken);

            // Try to take from highest priority channel first
            for (int i = _priorityChannels.Length - 1; i >= 0; i--)
            {
                var channel = _priorityChannels[i];
                
                if (channel.Reader.TryRead(out var item))
                {
                    Interlocked.Decrement(ref _currentCount);
                    return item;
                }
            }
        }

        throw new OperationCanceledException();
    }

    public void Clear()
    {
        foreach (var channel in _priorityChannels)
        {
            while (channel.Reader.TryRead(out _))
            {
                Interlocked.Decrement(ref _currentCount);
            }
        }
    }
}
