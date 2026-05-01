// ──────────────────────────────────────────────────────────────────────────────
// AURA-AUDIT v11.0: ZERO-COPY ARROW INTEGRATION (METRICS-FIXED)
// ──────────────────────────────────────────────────────────────────────────────

use core_affinity;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use std::io::{self, Write};

use arrow::array::{Int32Array, StringArray};
use arrow::record_batch::RecordBatch;
use arrow::datatypes::{Schema, Field, DataType};
use arrow::error::ArrowError;

// ──────────────────────────────────────────────────────────────────────────────
// CONFIGURATION CONSTANTS
// ──────────────────────────────────────────────────────────────────────────────
const POOL_A_CORES: std::ops::Range<usize> = 0..4;
const POOL_B_CORES: std::ops::Range<usize> = 4..32;
const SCAN_ITERATIONS: u64 = 10_000;
const LOG_PREFIX: &str = "AURA-AUDIT";

// Global program start time for relative timestamps
lazy_static::lazy_static! {
    static ref PROGRAM_START: Instant = Instant::now();
}

// ──────────────────────────────────────────────────────────────────────────────
// LOGGING UTILITIES (Thread-Safe, Relative Timestamps)
// ──────────────────────────────────────────────────────────────────────────────
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LogLevel {
    Info,
    Warn,
    Error,
}

impl LogLevel {
    fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Info => "[INFO]",
            LogLevel::Warn => "[WARN]",
            LogLevel::Error => "[ERROR]",
        }
    }
}

struct Logger {
    stdout: Mutex<io::Stdout>,
}

impl Logger {
    fn new() -> Self {
        Self { stdout: Mutex::new(io::stdout()) }
    }

    fn log(&self, level: LogLevel, pool: &str, core_id: usize, message: &str) {
        let mut handle = self.stdout.lock().unwrap_or_else(|e| e.into_inner());
        // FIX: Relative milliseconds since program start
        let elapsed_ms = PROGRAM_START.elapsed().as_millis();
        
        let _ = writeln!(
            handle,
            "{} T+{}ms {} | Core {:2} | {:12} | {}",
            LOG_PREFIX, elapsed_ms, level.as_str(), core_id, pool, message
        );
        let _ = handle.flush();
    }
}

static mut GLOBAL_LOGGER: Option<Logger> = None;
fn get_logger() -> &'static Logger {
    unsafe { GLOBAL_LOGGER.as_ref().expect("Logger not initialized") }
}

// ──────────────────────────────────────────────────────────────────────────────
// DATA SCHEMA (Arrow)
// ──────────────────────────────────────────────────────────────────────────────
fn create_forensic_schema() -> Result<Schema, ArrowError> {
    Ok(Schema::new(vec![
        Field::new("tx_id", DataType::Int32, false),
        Field::new("entity", DataType::Utf8, false),
        Field::new("timestamp_ns", DataType::UInt64, false),
        Field::new("risk_score", DataType::Float32, false),
    ]))
}

fn create_sample_batch(schema: &Schema) -> Result<RecordBatch, ArrowError> {
    let tx_ids = Int32Array::from(vec![101, 102, 103, 104, 105]);
    let entities = StringArray::from(vec![
        "Shell_A", "Capital_B", "Trust_C", "Holding_D", "Offshore_E",
    ]);
    let timestamps = arrow::array::UInt64Array::from(vec![
        1704067200000000000, 1704067201000000000, 1704067202000000000,
        1704067203000000000, 1704067204000000000,
    ]);
    let risk_scores = arrow::array::Float32Array::from(vec![0.12, 0.89, 0.45, 0.67, 0.23]);

    RecordBatch::try_new(
        Arc::new(schema.clone()),
        vec![
            Arc::new(tx_ids),
            Arc::new(entities),
            Arc::new(timestamps),
            Arc::new(risk_scores),
        ],
    )
}

// ──────────────────────────────────────────────────────────────────────────────
// THREAD POOL EXECUTORS (FIXED METRICS)
// ──────────────────────────────────────────────────────────────────────────────
struct PoolMetrics {
    total_evaluations: u64,
    high_risk_count: u64,
    start_time: Instant,
}

impl PoolMetrics {
    fn new() -> Self {
        Self { 
            total_evaluations: 0, 
            high_risk_count: 0,
            start_time: Instant::now() 
        }
    }

    fn record_evaluation(&mut self, is_high_risk: bool) {
        self.total_evaluations += 1;
        if is_high_risk { self.high_risk_count += 1; }
    }

    fn evaluations_per_sec(&self) -> f64 {
        let elapsed = self.start_time.elapsed();
        // FIX: Prevent division by near-zero; minimum 1ms for stable rates
        let elapsed_secs = elapsed.as_secs_f64().max(0.001);
        self.total_evaluations as f64 / elapsed_secs
    }

    fn high_risk_ratio(&self) -> f64 {
        if self.total_evaluations == 0 { 0.0 } 
        else { self.high_risk_count as f64 / self.total_evaluations as f64 }
    }
}

/// Pool A: I/O Marshalling
fn run_io_pool(
    core_id: usize,
    batch: Arc<RecordBatch>,
    shutdown: Arc<AtomicBool>,
    logger: &'static Logger,
) {
    logger.log(LogLevel::Info, "POOL_A", core_id, "I/O Marshaller Active");
    
    let mut metrics = PoolMetrics::new();
    
    while !shutdown.load(Ordering::Relaxed) {
        let num_rows = batch.num_rows();
        let _schema = batch.schema();
        let _columns = batch.columns();
        
        metrics.record_evaluation(false); // I/O threads don't evaluate risk
        
        thread::yield_now();
    }
    
    logger.log(
        LogLevel::Info,
        "POOL_A",
        core_id,
        &format!(
            "I/O Complete | Rows: {} | Cols: {} | Ops: {} | Rate: {:.0} ops/sec",
            batch.num_rows(),
            batch.num_columns(),
            metrics.total_evaluations,
            metrics.evaluations_per_sec()
        ),
    );
}

/// Pool B: Forensic Compute Engine (FIXED METRICS)
fn run_compute_pool(
    core_id: usize,
    batch: Arc<RecordBatch>,
    shutdown: Arc<AtomicBool>,
    logger: &'static Logger,
) {
    logger.log(LogLevel::Info, "POOL_B", core_id, "Forensic Compute Engine Active");
    
    let mut metrics = PoolMetrics::new();
    let mut iteration: u64 = 0;
    
    // Pre-extract risk column for zero-copy access
    let risk_col = batch.column(3)
        .as_any()
        .downcast_ref::<arrow::array::Float32Array>()
        .expect("Column 3 must be Float32Array");
    let risk_values = risk_col.values();
    
    while !shutdown.load(Ordering::Relaxed) && iteration < SCAN_ITERATIONS {
        // Zero-copy risk evaluation
        for &risk in risk_values.iter() {
            let is_high_risk = risk > 0.5;
            metrics.record_evaluation(is_high_risk);
        }
        
        iteration += 1;
        
        if iteration % 1000 == 0 {
            thread::yield_now();
        }
    }
    
    // FIX: Clear, accurate metrics reporting
    logger.log(
        LogLevel::Info,
        "POOL_B",
        core_id,
        &format!(
            "Compute Complete | Iterations: {} | Evaluations: {} | High-Risk: {} ({:.1}%) | Rate: {:.0} evals/sec",
            iteration,
            metrics.total_evaluations,
            metrics.high_risk_count,
            metrics.high_risk_ratio() * 100.0,
            metrics.evaluations_per_sec()
        ),
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// MAIN EXECUTION
// ──────────────────────────────────────────────────────────────────────────────
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger
    let logger = Logger::new();
    unsafe { GLOBAL_LOGGER = Some(logger) };
    let logger = get_logger();

    logger.log(LogLevel::Info, "INIT", 0, "AURA-AUDIT v11.0 Starting");

    // Core affinity setup
    let core_ids = match core_affinity::get_core_ids() {
        Some(ids) => ids,
        None => {
            logger.log(LogLevel::Error, "INIT", 0, "Failed to retrieve core IDs");
            return Err("Core affinity initialization failed".into());
        }
    };

    let total_cores = core_ids.len();
    logger.log(LogLevel::Info, "INIT", 0, &format!("Detected {} logical cores", total_cores));

    // Arrow data preparation
    let schema = create_forensic_schema()?;
    let batch = Arc::new(create_sample_batch(&schema)?);
    
    logger.log(
        LogLevel::Info,
        "ARROW",
        0,
        &format!(
            "RecordBatch: {} rows × {} cols | Memory: ~{} KB",
            batch.num_rows(),
            batch.num_columns(),
            batch.get_array_memory_size() / 1024
        ),
    );

    // Shutdown signal
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let mut handles = Vec::with_capacity(total_cores);

    // Spawn threads
    for core_id in core_ids {
        let cid = core_id.id;
        let batch_clone = Arc::clone(&batch);
        let shutdown_clone = Arc::clone(&shutdown_flag);
        
        if !core_affinity::set_for_current(core_id) {
            logger.log(LogLevel::Warn, "AFFINITY", cid, "Failed to pin thread");
        }

        let handle = if POOL_A_CORES.contains(&cid) {
            thread::spawn(move || run_io_pool(cid, batch_clone, shutdown_clone, logger))
        } else if POOL_B_CORES.contains(&cid) {
            thread::spawn(move || run_compute_pool(cid, batch_clone, shutdown_clone, logger))
        } else {
            logger.log(LogLevel::Warn, "TOPOLOGY", cid, "Core not assigned to pool");
            continue;
        };

        handles.push(handle);
    }

    logger.log(LogLevel::Info, "STATUS", 0, "Pipeline active. Press ENTER to shutdown.");

    // Wait for input
    let mut input = String::new();
    let _ = io::stdin().read_line(&mut input);

    // Graceful shutdown
    logger.log(LogLevel::Info, "SHUTDOWN", 0, "Initiating graceful termination");
    shutdown_flag.store(true, Ordering::Relaxed);

    for (idx, handle) in handles.into_iter().enumerate() {
        if let Err(e) = handle.join() {
            logger.log(LogLevel::Error, "JOIN", idx, &format!("Thread join failed: {:?}", e));
        }
    }

    // Final summary
    logger.log(
        LogLevel::Info,
        "SUMMARY",
        0,
        &format!(
            "Complete | Batch: {}×{} | Memory: {} KB | Zero-copy: YES",
            batch.num_rows(),
            batch.num_columns(),
            batch.get_array_memory_size() / 1024
        ),
    );

    println!("--- {} v11.0: ZERO-COPY ARROW INTEGRATION COMPLETE ---", LOG_PREFIX);
    Ok(())
}
