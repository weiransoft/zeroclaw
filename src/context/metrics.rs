//! Context management performance monitoring
//! 
//! This module provides performance metrics collection and reporting:
//! - Operation timing
//! - Cache hit rate tracking
//! - Context size monitoring
//! - Performance report generation

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Performance metrics for context management
#[derive(Debug, Default)]
pub struct ContextMetrics {
    /// Operation counters (operation name -> count)
    operation_counts: Arc<Mutex<HashMap<String, AtomicU64>>>,
    
    /// Operation durations in milliseconds (operation name -> total ms)
    operation_durations: Arc<Mutex<HashMap<String, AtomicU64>>>,
    
    /// Cache hits
    cache_hits: Arc<AtomicU64>,
    
    /// Cache misses
    cache_misses: Arc<AtomicU64>,
    
    /// Context sizes in tokens (context_id -> token count)
    context_sizes: Arc<Mutex<HashMap<String, usize>>>,
}

impl ContextMetrics {
    /// Create a new metrics collector
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Record the start of an operation
    pub fn start_operation(&self, _operation: &str) -> Instant {
        Instant::now()
    }
    
    /// Record the end of an operation and update metrics
    pub fn end_operation(&self, operation: &str, start: Instant) {
        let duration_ms = start.elapsed().as_millis() as u64;
        
        // Update operation count
        {
            let mut counts = self.operation_counts.lock().unwrap();
            counts
                .entry(operation.to_string())
                .or_insert_with(|| AtomicU64::new(0))
                .fetch_add(1, Ordering::SeqCst);
        }
        
        // Update operation duration
        {
            let mut durations = self.operation_durations.lock().unwrap();
            durations
                .entry(operation.to_string())
                .or_insert_with(|| AtomicU64::new(0))
                .fetch_add(duration_ms, Ordering::SeqCst);
        }
        
        tracing::debug!("Operation completed: {}, duration: {}ms", operation, duration_ms);
    }
    
    /// Record a cache hit
    pub fn record_cache_hit(&self) {
        self.cache_hits.fetch_add(1, Ordering::SeqCst);
    }
    
    /// Record a cache miss
    pub fn record_cache_miss(&self) {
        self.cache_misses.fetch_add(1, Ordering::SeqCst);
    }
    
    /// Update context size
    pub fn update_context_size(&self, context_id: &str, token_count: usize) {
        let mut sizes = self.context_sizes.lock().unwrap();
        sizes.insert(context_id.to_string(), token_count);
    }
    
    /// Get operation count
    pub fn get_operation_count(&self, operation: &str) -> u64 {
        let counts = self.operation_counts.lock().unwrap();
        counts
            .get(operation)
            .map(|c| c.load(Ordering::SeqCst))
            .unwrap_or(0)
    }
    
    /// Get average operation duration in milliseconds
    pub fn get_average_duration(&self, operation: &str) -> f64 {
        let counts = self.operation_counts.lock().unwrap();
        let durations = self.operation_durations.lock().unwrap();
        
        let count = counts
            .get(operation)
            .map(|c| c.load(Ordering::SeqCst))
            .unwrap_or(0);
        
        let total_duration = durations
            .get(operation)
            .map(|d| d.load(Ordering::SeqCst))
            .unwrap_or(0);
        
        if count == 0 {
            0.0
        } else {
            total_duration as f64 / count as f64
        }
    }
    
    /// Calculate cache hit rate
    pub fn cache_hit_rate(&self) -> f64 {
        let hits = self.cache_hits.load(Ordering::SeqCst) as f64;
        let misses = self.cache_misses.load(Ordering::SeqCst) as f64;
        
        let total = hits + misses;
        if total == 0.0 {
            0.0
        } else {
            hits / total
        }
    }
    
    /// Get context size
    pub fn get_context_size(&self, context_id: &str) -> Option<usize> {
        let sizes = self.context_sizes.lock().unwrap();
        sizes.get(context_id).copied()
    }
    
    /// Generate performance report
    pub fn generate_report(&self) -> String {
        let mut report = String::from("=== Context Performance Report ===\n\n");
        
        // Operation statistics
        report.push_str("Operation Statistics:\n");
        let counts = self.operation_counts.lock().unwrap();
        let durations = self.operation_durations.lock().unwrap();
        
        for (operation, count) in counts.iter() {
            let count = count.load(Ordering::SeqCst);
            let avg_duration = durations
                .get(operation)
                .map(|d| d.load(Ordering::SeqCst) as f64 / count as f64)
                .unwrap_or(0.0);
            
            report.push_str(&format!(
                "  {}: {} calls, avg {:.2}ms\n",
                operation, count, avg_duration
            ));
        }
        
        report.push('\n');
        
        // Cache statistics
        report.push_str("Cache Statistics:\n");
        let hits = self.cache_hits.load(Ordering::SeqCst);
        let misses = self.cache_misses.load(Ordering::SeqCst);
        let hit_rate = self.cache_hit_rate();
        
        report.push_str(&format!("  - Hits: {}\n", hits));
        report.push_str(&format!("  - Misses: {}\n", misses));
        report.push_str(&format!("  - Hit Rate: {:.2}%\n", hit_rate * 100.0));
        
        report.push('\n');
        
        // Context size statistics
        report.push_str("Context Sizes:\n");
        let sizes = self.context_sizes.lock().unwrap();
        for (context_id, token_count) in sizes.iter() {
            report.push_str(&format!("  - {}: {} tokens\n", context_id, token_count));
        }
        
        report.push_str("\n=================================\n");
        
        report
    }
    
    /// Reset all metrics
    pub fn reset(&self) {
        {
            let mut counts = self.operation_counts.lock().unwrap();
            counts.clear();
        }
        {
            let mut durations = self.operation_durations.lock().unwrap();
            durations.clear();
        }
        self.cache_hits.store(0, Ordering::SeqCst);
        self.cache_misses.store(0, Ordering::SeqCst);
        {
            let mut sizes = self.context_sizes.lock().unwrap();
            sizes.clear();
        }
        
        tracing::info!("Context metrics reset");
    }
}

/// Performance monitor guard for automatic timing
pub struct OperationGuard<'a> {
    metrics: &'a ContextMetrics,
    operation: String,
    start: Instant,
}

impl<'a> OperationGuard<'a> {
    /// Create a new operation guard
    pub fn new(metrics: &'a ContextMetrics, operation: &str) -> Self {
        let start = metrics.start_operation(operation);
        Self {
            metrics,
            operation: operation.to_string(),
            start,
        }
    }
}

impl<'a> Drop for OperationGuard<'_> {
    fn drop(&mut self) {
        self.metrics.end_operation(&self.operation, self.start);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;
    
    #[test]
    fn test_metrics_creation() {
        let metrics = ContextMetrics::new();
        assert_eq!(metrics.cache_hit_rate(), 0.0);
    }
    
    #[test]
    fn test_operation_timing() {
        let metrics = ContextMetrics::new();
        
        // Simulate an operation
        let start = metrics.start_operation("test_op");
        thread::sleep(Duration::from_millis(10));
        metrics.end_operation("test_op", start);
        
        assert_eq!(metrics.get_operation_count("test_op"), 1);
        assert!(metrics.get_average_duration("test_op") >= 10.0);
    }
    
    #[test]
    fn test_cache_tracking() {
        let metrics = ContextMetrics::new();
        
        metrics.record_cache_hit();
        metrics.record_cache_hit();
        metrics.record_cache_miss();
        
        assert_eq!(metrics.cache_hit_rate(), 2.0 / 3.0);
    }
    
    #[test]
    fn test_context_size_tracking() {
        let metrics = ContextMetrics::new();
        
        metrics.update_context_size("ctx-1", 1000);
        metrics.update_context_size("ctx-2", 2000);
        
        assert_eq!(metrics.get_context_size("ctx-1"), Some(1000));
        assert_eq!(metrics.get_context_size("ctx-2"), Some(2000));
        assert_eq!(metrics.get_context_size("ctx-3"), None);
    }
    
    #[test]
    fn test_operation_guard() {
        let metrics = ContextMetrics::new();
        
        {
            let _guard = OperationGuard::new(&metrics, "guarded_op");
            thread::sleep(Duration::from_millis(5));
        }
        
        assert_eq!(metrics.get_operation_count("guarded_op"), 1);
        assert!(metrics.get_average_duration("guarded_op") >= 5.0);
    }
    
    #[test]
    fn test_metrics_reset() {
        let metrics = ContextMetrics::new();
        
        metrics.record_cache_hit();
        metrics.update_context_size("ctx-1", 1000);
        
        let start = metrics.start_operation("test");
        metrics.end_operation("test", start);
        
        metrics.reset();
        
        assert_eq!(metrics.cache_hit_rate(), 0.0);
        assert_eq!(metrics.get_operation_count("test"), 0);
        assert_eq!(metrics.get_context_size("ctx-1"), None);
    }
    
    #[test]
    fn test_report_generation() {
        let metrics = ContextMetrics::new();
        
        metrics.record_cache_hit();
        metrics.update_context_size("ctx-1", 1000);
        
        let start = metrics.start_operation("report_test");
        metrics.end_operation("report_test", start);
        
        let report = metrics.generate_report();
        
        assert!(report.contains("Context Performance Report"));
        assert!(report.contains("Cache Statistics"));
        assert!(report.contains("Operation Statistics"));
    }
}
