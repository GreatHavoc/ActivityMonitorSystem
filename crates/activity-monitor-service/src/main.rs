//! Activity Monitor service host — port of ActivityMonitor.Service
//! (Program.cs + ActivityMonitorService.ExecuteAsync). Console mode by default,
//! `--service` runs under the Windows service dispatcher (same CLI as C#).
//! Fix: logs go to <exe_dir>/Logs instead of the hardcoded
//! C:\ActivityMonitor\publish\Logs; the DB path is resolved against the CWD,
//! as in C# Path.GetFullPath(Storage.DatabasePath).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use activity_monitor_core::config::ActivityMonitorSettings;
use activity_monitor_core::db::ActivityDatabase;
use activity_monitor_core::models::{ActivityEvent, InferenceRequest, RequestPriority};
use activity_monitor_core::ollama::OllamaClient;
use activity_monitor_core::queue::RequestQueueManager;
use activity_monitor_core::sensors;
use jiff::{Timestamp, Unit};
use uuid::Uuid;

/// Hardcoded in C# (ActivityMonitorService): periodic capture every 3 minutes,
/// 10 s throttle on event-based captures.
const PERIODIC_CAPTURE_INTERVAL: Duration = Duration::from_secs(3 * 60);
const MIN_EVENT_CAPTURE_INTERVAL: Duration = Duration::from_secs(10);
const ERROR_BACKOFF: Duration = Duration::from_secs(5);

static SETTINGS: OnceLock<ActivityMonitorSettings> = OnceLock::new();

fn main() {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    let _guard = activity_monitor_core::logging::init(&exe_dir.join("Logs")).expect("init logging");
    let settings = ActivityMonitorSettings::load_or_default(exe_dir.join("appsettings.json"));
    tracing::info!(
        sampling_interval_s = settings.sampling_interval_seconds,
        db = %settings.storage.database_path,
        "Starting Activity Monitor"
    );
    let as_service = std::env::args().any(|a| a == "--service");
    SETTINGS.set(settings).ok();

    if as_service {
        #[cfg(windows)]
        {
            service_mode::run();
            return;
        }
        #[cfg(not(windows))]
        {
            tracing::error!("--service is only supported on Windows; running as console");
        }
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let code = rt.block_on(console_main());
    std::process::exit(code);
}

async fn console_main() -> i32 {
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel2 = Arc::clone(&cancel);
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        cancel2.store(true, Ordering::Relaxed);
    });
    let settings = SETTINGS.get().expect("settings").clone();
    run_loop(&settings, cancel).await
}

/// Port of ExecuteAsync. Returns the process exit code (C# Environment.Exit(1)
/// on a fatal startup error, 0 on graceful stop).
async fn run_loop(settings: &ActivityMonitorSettings, cancel: Arc<AtomicBool>) -> i32 {
    tracing::info!("Activity Monitor Service starting...");

    // C# Path.GetFullPath(Storage.DatabasePath): relative paths resolve against CWD.
    let db_path = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(&settings.storage.database_path);
    let db = match ActivityDatabase::open(&db_path).and_then(|d| d.initialize().map(|_| d)) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!(error = %e, "Fatal error in Activity Monitor Service");
            return 1;
        }
    };
    tracing::info!(db = %db_path.display(), "Initializing database");
    let db = Arc::new(Mutex::new(db));
    let ollama = Arc::new(OllamaClient::new(settings));
    let queue = Arc::new(RequestQueueManager::new(settings, ollama, Arc::clone(&db)));
    queue.start();
    tracing::info!("Activity Monitor Service started successfully");

    // Compaction was dead code in C# (never called); the port wires it in:
    // run once at startup, then every Storage.CompactionIntervalHours.
    let compact_interval = Duration::from_secs((settings.storage.compaction_interval_hours.max(1) as u64) * 3600);
    let mut next_compaction = Instant::now(); // startup run
    if let Err(e) = db.lock().unwrap().compact_old_events(settings.storage.max_event_age_days) {
        tracing::error!(error = %e, "Error during startup compaction");
    }

    let sampling = Duration::from_secs(settings.sampling_interval_seconds.max(0) as u64);
    let mut last_periodic: Option<Instant> = None; // DateTime.MinValue -> first iteration triggers
    let mut last_capture: Option<Instant> = None;
    let mut last_focused: Option<String> = None;
    let mut was_idle = false;

    loop {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if let Err(e) = iteration(
            settings,
            &queue,
            &db,
            &cancel,
            &mut last_periodic,
            &mut last_capture,
            &mut last_focused,
            &mut was_idle,
            &mut next_compaction,
            compact_interval,
        )
        .await
        {
            tracing::error!(error = %e, "Error in activity monitoring loop");
            tokio::select! {
                _ = wait_cancel(&cancel) => break,
                _ = tokio::time::sleep(ERROR_BACKOFF) => {}
            }
        }
        tokio::select! {
            _ = wait_cancel(&cancel) => break,
            _ = tokio::time::sleep(sampling) => {}
        }
    }

    tracing::info!("Activity Monitor Service stopping...");
    queue.stop().await;
    tracing::info!("Activity Monitor Service stopped");
    0
}

/// One pass of the C# while-body. Err triggers the 5 s error backoff.
#[allow(clippy::too_many_arguments)]
async fn iteration(
    settings: &ActivityMonitorSettings,
    queue: &Arc<RequestQueueManager>,
    db: &Arc<Mutex<ActivityDatabase>>,
    cancel: &AtomicBool,
    last_periodic: &mut Option<Instant>,
    last_capture: &mut Option<Instant>,
    last_focused: &mut Option<String>,
    was_idle: &mut bool,
    next_compaction: &mut Instant,
    compact_interval: Duration,
) -> Result<(), String> {
    // Periodic capture for continuous monitoring
    if last_periodic.map_or(true, |t| t.elapsed() >= PERIODIC_CAPTURE_INTERVAL) {
        tracing::info!("Triggering periodic capture for continuous monitoring");
        trigger_capture(queue, "periodic_monitoring", RequestPriority::Normal, last_capture);
        *last_periodic = Some(Instant::now());
    }

    // Idle state + idle-resume trigger
    let idle = sensors::idle_state(settings.idle_threshold_seconds);
    let is_idle = idle.is_idle;
    if *was_idle && !is_idle {
        tracing::info!("User resumed from idle");
        if settings.capture_settings.trigger_on_idle_resume {
            trigger_capture(queue, "idle_resume", RequestPriority::High, last_capture);
        }
    }
    *was_idle = is_idle;

    if !is_idle {
        if let Some(focus) = sensors::current_focus() {
            let window_key = format!("{}:{}", focus.process_id, focus.window_title);
            if last_focused.as_deref() != Some(window_key.as_str()) {
                tracing::info!(
                    process = %focus.process_name,
                    title = %focus.window_title,
                    "Focus changed to process-title"
                );
                let event = ActivityEvent {
                    id: 0,
                    timestamp: Timestamp::now(),
                    event_type: "focus_change".into(),
                    process_id: Some(focus.process_id),
                    process_name: Some(focus.process_name),
                    window_title: Some(focus.window_title),
                    is_idle: false,
                    idle_duration_seconds: None,
                    metadata: None,
                };
                db.lock()
                    .unwrap()
                    .save_activity_event(&event)
                    .map_err(|e| e.to_string())?;
                if settings.capture_settings.trigger_on_focus_change {
                    trigger_capture(queue, "focus_change", RequestPriority::High, last_capture);
                }
                *last_focused = Some(window_key);
            }
        }
    } else {
        let event = ActivityEvent {
            id: 0,
            timestamp: Timestamp::now(),
            event_type: "idle".into(),
            process_id: None,
            process_name: None,
            window_title: None,
            is_idle: true,
            idle_duration_seconds: Some(idle.idle_duration.total(Unit::Second).unwrap_or(0.0) as i64),
            metadata: None,
        };
        db.lock()
            .unwrap()
            .save_activity_event(&event)
            .map_err(|e| e.to_string())?;
    }

    // Compaction (wired in the port; dead in C#)
    if Instant::now() >= *next_compaction {
        match db.lock().unwrap().compact_old_events(settings.storage.max_event_age_days) {
            Ok(deleted) => tracing::info!(deleted, "Compaction finished"),
            Err(e) => tracing::error!(error = %e, "Error during compaction"),
        }
        *next_compaction = Instant::now() + compact_interval;
    }

    let _ = cancel;
    Ok(())
}

/// Port of TriggerCaptureAsync: 10 s throttle on event-based captures only.
fn trigger_capture(
    queue: &Arc<RequestQueueManager>,
    reason: &str,
    priority: RequestPriority,
    last_capture: &mut Option<Instant>,
) {
    if reason != "periodic_monitoring" {
        if let Some(t) = *last_capture {
            if t.elapsed() < MIN_EVENT_CAPTURE_INTERVAL {
                tracing::debug!("Skipping capture, too soon since last capture");
                return;
            }
        }
    }
    tracing::info!(reason, "Triggering screen capture");
    let request = InferenceRequest {
        id: Uuid::new_v4(),
        timestamp: Timestamp::now(),
        trigger_reason: reason.into(),
        priority,
        captured_frames: None,
        metadata: None,
    };
    queue.enqueue(request);
    if reason != "periodic_monitoring" {
        *last_capture = Some(Instant::now());
    }
}

async fn wait_cancel(cancel: &AtomicBool) {
    while !cancel.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[cfg(windows)]
mod service_mode {
    //! windows-service 0.8.1 integration, mirroring Program.cs
    //! `args.Contains("--service")` + AddWindowsService(ServiceName =
    //! "Activity Monitor Service").

    use std::ffi::OsString;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use windows_service::service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    };
    use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
    use windows_service::{define_windows_service, service_dispatcher};

    use super::{run_loop, SETTINGS};

    define_windows_service!(ffi_service_main, service_main);

    pub fn run() {
        service_dispatcher::start("Activity Monitor Service", ffi_service_main)
            .expect("service dispatcher");
    }

    fn service_main(_args: Vec<OsString>) {
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel2 = Arc::clone(&cancel);
        let event_handler = move |control: ServiceControl| -> ServiceControlHandlerResult {
            match control {
                ServiceControl::Stop => {
                    cancel2.store(true, Ordering::Relaxed);
                    ServiceControlHandlerResult::NoError
                }
                _ => ServiceControlHandlerResult::NotImplemented,
            }
        };
        let status_handle = match service_control_handler::register("Activity Monitor Service", event_handler) {
            Ok(h) => h,
            Err(e) => {
                tracing::error!(error = %e, "Could not register service control handler");
                std::process::exit(1);
            }
        };
        let running = ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::from_secs(5),
            process_id: None,
        };
        if let Err(e) = status_handle.set_service_status(running.clone()) {
            tracing::error!(error = %e, "Could not set service status to Running");
        }

        let settings = SETTINGS.get().expect("settings").clone();
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        let code = rt.block_on(run_loop(&settings, cancel));

        let stopped = ServiceStatus {
            current_state: ServiceState::Stopped,
            exit_code: ServiceExitCode::Win32(code as u32),
            ..running
        };
        let _ = status_handle.set_service_status(stopped);
        std::process::exit(code);
    }
}
