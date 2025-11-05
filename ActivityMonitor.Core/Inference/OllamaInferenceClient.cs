using ActivityMonitor.Common.Configuration;
using ActivityMonitor.Common.Models;
using Microsoft.Extensions.Logging;
using Microsoft.Extensions.Options;
using System.Net.Http.Json;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace ActivityMonitor.Core.Inference;

/// <summary>
/// Client for Ollama server with Qwen3-VL-2B model
/// Handles multimodal inference requests using Ollama API
/// </summary>
public class OllamaInferenceClient
{
    private readonly HttpClient _httpClient;
    private readonly ILogger<OllamaInferenceClient> _logger;
    private readonly ActivityMonitorSettings _settings;

    public OllamaInferenceClient(
        HttpClient httpClient,
        ILogger<OllamaInferenceClient> logger,
        IOptions<ActivityMonitorSettings> settings)
    {
        _httpClient = httpClient;
        _logger = logger;
        _settings = settings.Value;
        
        _httpClient.BaseAddress = new Uri(_settings.OllamaEndpoint);
        _httpClient.Timeout = Timeout.InfiniteTimeSpan; // No timeout - wait forever
    }

    /// <summary>
    /// Analyzes captured frames using Qwen3-VL via Ollama
    /// </summary>
    public async Task<InferenceResult?> AnalyzeFramesAsync(
        List<byte[]> frames, 
        CancellationToken cancellationToken)
    {
        try
        {
            _logger.LogInformation("Sending {FrameCount} frames to Ollama for analysis", frames.Count);

            // Prepare request with vision support
            var request = new OllamaGenerateRequest
            {
                Model = _settings.OllamaModel,
                Prompt = BuildPrompt(),
                Images = ConvertFramesToBase64(frames),
                Stream = false,
                Options = new OllamaOptions
                {
                    Temperature = 0.1,
                    TopP = 0.9,
                    NumPredict = 512
                }
            };

            var response = await _httpClient.PostAsJsonAsync(
                "/api/generate", 
                request, 
                cancellationToken);

            if (!response.IsSuccessStatusCode)
            {
                var error = await response.Content.ReadAsStringAsync(cancellationToken);
                _logger.LogError("Ollama request failed: {StatusCode} - {Error}", 
                    response.StatusCode, error);
                return null;
            }

            var ollamaResponse = await response.Content.ReadFromJsonAsync<OllamaGenerateResponse>(
                cancellationToken: cancellationToken);

            if (ollamaResponse == null || string.IsNullOrEmpty(ollamaResponse.Response))
            {
                _logger.LogWarning("Empty response from Ollama");
                return null;
            }

            _logger.LogDebug("Ollama response: {Response}", ollamaResponse.Response);

            return ParseResponse(ollamaResponse.Response);
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Error during Ollama inference");
            return null;
        }
    }

    private string BuildPrompt()
    {
        return @"Analyze the screen and extract user activity details in JSON:
{
  ""activity_label"": ""brief category"",
  ""application"": ""app name"",
  ""content_type"": ""code/web/document/video/chat"",
  ""topic"": ""specific subject"",
  ""action"": ""reading/writing/browsing"",
  ""summary"": ""what user is doing"",
  ""visible_text"": ""key visible text or URLs"",
  ""confidence"": 0.9
}";
    }

    private List<string> ConvertFramesToBase64(List<byte[]> frames)
    {
        // Use only 1 frame to stay within token limits
        var base64Frames = new List<string>();

        if (frames.Count > 0)
        {
            // Take the middle frame for best representation
            var middleIndex = frames.Count / 2;
            var base64 = Convert.ToBase64String(frames[middleIndex]);
            base64Frames.Add(base64);
        }

        _logger.LogDebug("Converted {OriginalCount} frames to 1 sample", frames.Count);

        return base64Frames;
    }

    private InferenceResult? ParseResponse(string content)
    {
        try
        {
            // Try to extract JSON from the response
            // Ollama may wrap JSON in markdown code blocks
            var jsonContent = ExtractJson(content);

            var jsonDoc = JsonDocument.Parse(jsonContent);
            var root = jsonDoc.RootElement;

            return new InferenceResult
            {
                ProcessedAt = DateTime.UtcNow,
                ActivityLabel = root.GetProperty("activity_label").GetString() ?? "Unknown",
                Application = root.TryGetProperty("application", out var app) ? app.GetString() ?? "" : "",
                ContentType = root.TryGetProperty("content_type", out var ct) ? ct.GetString() ?? "" : "",
                Topic = root.TryGetProperty("topic", out var topic) ? topic.GetString() ?? "" : "",
                Action = root.TryGetProperty("action", out var action) ? action.GetString() ?? "" : "",
                Summary = root.GetProperty("summary").GetString() ?? "",
                VisibleText = root.TryGetProperty("visible_text", out var vt) ? vt.GetString() ?? "" : "",
                Confidence = root.TryGetProperty("confidence", out var conf) ? conf.GetDouble() : 0.0,
                RawResponse = content
            };
        }
        catch (Exception ex)
        {
            _logger.LogWarning(ex, "Failed to parse JSON response, using raw content");
            
            // Fallback: use the raw content as summary
            return new InferenceResult
            {
                ProcessedAt = DateTime.UtcNow,
                ActivityLabel = "Activity",
                Summary = content.Length > 500 ? content.Substring(0, 500) + "..." : content,
                Confidence = 0.5,
                RawResponse = content
            };
        }
    }

    private string ExtractJson(string content)
    {
        // Remove markdown code blocks if present
        var trimmed = content.Trim();
        
        if (trimmed.StartsWith("```json"))
        {
            var startIndex = trimmed.IndexOf('\n') + 1;
            var endIndex = trimmed.LastIndexOf("```");
            if (startIndex > 0 && endIndex > startIndex)
            {
                return trimmed.Substring(startIndex, endIndex - startIndex).Trim();
            }
        }
        else if (trimmed.StartsWith("```"))
        {
            var startIndex = trimmed.IndexOf('\n') + 1;
            var endIndex = trimmed.LastIndexOf("```");
            if (startIndex > 0 && endIndex > startIndex)
            {
                return trimmed.Substring(startIndex, endIndex - startIndex).Trim();
            }
        }

        // Try to find JSON object boundaries
        var jsonStart = trimmed.IndexOf('{');
        var jsonEnd = trimmed.LastIndexOf('}');
        
        if (jsonStart >= 0 && jsonEnd > jsonStart)
        {
            return trimmed.Substring(jsonStart, jsonEnd - jsonStart + 1);
        }

        return trimmed;
    }

    /// <summary>
    /// Checks if Ollama is available and the model is loaded
    /// </summary>
    public async Task<bool> CheckHealthAsync(CancellationToken cancellationToken)
    {
        try
        {
            var response = await _httpClient.GetAsync("/api/tags", cancellationToken);
            
            if (!response.IsSuccessStatusCode)
            {
                _logger.LogWarning("Ollama health check failed: {StatusCode}", response.StatusCode);
                return false;
            }

            var tags = await response.Content.ReadFromJsonAsync<OllamaTagsResponse>(cancellationToken: cancellationToken);
            
            if (tags?.Models == null)
            {
                return false;
            }

            var modelExists = tags.Models.Any(m => 
                m.Name.Contains(_settings.OllamaModel, StringComparison.OrdinalIgnoreCase));

            if (!modelExists)
            {
                _logger.LogWarning("Model {ModelName} not found in Ollama. Available models: {Models}", 
                    _settings.OllamaModel, 
                    string.Join(", ", tags.Models.Select(m => m.Name)));
            }

            return modelExists;
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "Error checking Ollama health");
            return false;
        }
    }
}

#region Ollama API Models

public class OllamaGenerateRequest
{
    [JsonPropertyName("model")]
    public string Model { get; set; } = string.Empty;

    [JsonPropertyName("prompt")]
    public string Prompt { get; set; } = string.Empty;

    [JsonPropertyName("images")]
    public List<string>? Images { get; set; }

    [JsonPropertyName("stream")]
    public bool Stream { get; set; } = false;

    [JsonPropertyName("options")]
    public OllamaOptions? Options { get; set; }
}

public class OllamaOptions
{
    [JsonPropertyName("temperature")]
    public double Temperature { get; set; } = 0.7;

    [JsonPropertyName("top_p")]
    public double TopP { get; set; } = 0.9;

    [JsonPropertyName("num_predict")]
    public int NumPredict { get; set; } = 512;
}

public class OllamaGenerateResponse
{
    [JsonPropertyName("model")]
    public string Model { get; set; } = string.Empty;

    [JsonPropertyName("created_at")]
    public string CreatedAt { get; set; } = string.Empty;

    [JsonPropertyName("response")]
    public string Response { get; set; } = string.Empty;

    [JsonPropertyName("done")]
    public bool Done { get; set; }

    [JsonPropertyName("total_duration")]
    public long? TotalDuration { get; set; }

    [JsonPropertyName("load_duration")]
    public long? LoadDuration { get; set; }

    [JsonPropertyName("prompt_eval_duration")]
    public long? PromptEvalDuration { get; set; }

    [JsonPropertyName("eval_duration")]
    public long? EvalDuration { get; set; }
}

public class OllamaTagsResponse
{
    [JsonPropertyName("models")]
    public List<OllamaModel> Models { get; set; } = new();
}

public class OllamaModel
{
    [JsonPropertyName("name")]
    public string Name { get; set; } = string.Empty;

    [JsonPropertyName("modified_at")]
    public string ModifiedAt { get; set; } = string.Empty;

    [JsonPropertyName("size")]
    public long Size { get; set; }
}

#endregion
