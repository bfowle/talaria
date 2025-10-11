//! Custom tracing layer for audit logging

use super::{audit_entry, audit_exit, write_audit, AuditEventType};
use chrono::Utc;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;
use tracing::span::{Attributes, Id};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::LookupSpan;

/// Span timing information
#[derive(Debug, Clone)]
struct SpanTiming {
    start: Instant,
    module: String,
    function: String,
    file: String,
    line: u32,
    fields: HashMap<String, serde_json::Value>,
}

/// Custom tracing layer for audit logging
pub struct AuditLayer {
    spans: Mutex<HashMap<Id, SpanTiming>>,
    include_trace: bool,
}

impl AuditLayer {
    /// Create a new audit layer
    pub fn new(include_trace: bool) -> Self {
        Self {
            spans: Mutex::new(HashMap::new()),
            include_trace,
        }
    }
}

impl<S> Layer<S> for AuditLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, _ctx: Context<'_, S>) {
        let metadata = attrs.metadata();

        // Only track INFO level and above (skip DEBUG and TRACE unless configured)
        // In tracing: ERROR < WARN < INFO < DEBUG < TRACE (in terms of verbosity)
        // We want to include ERROR, WARN, and INFO by default
        if !self.include_trace && *metadata.level() > Level::INFO {
            return;
        }

        // Extract span information
        let module = metadata.module_path().unwrap_or("unknown");
        let function = metadata.name();
        let file = metadata.file().unwrap_or("unknown");
        let line = metadata.line().unwrap_or(0);

        // Extract fields
        let mut fields = HashMap::new();
        attrs.record(&mut FieldVisitor(&mut fields));

        let timing = SpanTiming {
            start: Instant::now(),
            module: module.to_string(),
            function: function.to_string(),
            file: file.to_string(),
            line,
            fields: fields.clone(),
        };

        self.spans.lock().unwrap().insert(id.clone(), timing);

        // Write entry audit log
        if super::is_audit_enabled() {
            let entry = audit_entry(module, function, file, line, json!(fields));
            write_audit(entry);
        }
    }

    fn on_close(&self, id: Id, _ctx: Context<'_, S>) {
        if let Some(timing) = self.spans.lock().unwrap().remove(&id) {
            let duration = timing.start.elapsed();

            // Write exit audit log
            if super::is_audit_enabled() {
                let mut data = timing.fields;
                data.insert("duration_ms".to_string(), json!(duration.as_millis()));

                let entry = audit_exit(
                    &timing.module,
                    &timing.function,
                    &timing.file,
                    timing.line,
                    json!(data),
                    Some(duration),
                );
                write_audit(entry);
            }
        }
    }

    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();

        // Only log WARN and ERROR events as audit events
        if metadata.level() <= &Level::WARN {
            let module = metadata.module_path().unwrap_or("unknown");
            let file = metadata.file().unwrap_or("unknown");
            let line = metadata.line().unwrap_or(0);

            // Extract event fields
            let mut fields = HashMap::new();
            event.record(&mut FieldVisitor(&mut fields));

            if super::is_audit_enabled() {
                let entry = super::AuditEntry {
                    timestamp: Utc::now(),
                    level: format!("{}", metadata.level()),
                    module: module.to_string(),
                    function: metadata.name().to_string(),
                    file: file.to_string(),
                    line,
                    event: if metadata.level() == &Level::ERROR {
                        AuditEventType::Error
                    } else {
                        AuditEventType::Data
                    },
                    data: json!(fields),
                    thread_id: format!("{:?}", std::thread::current().id()),
                    duration_ms: None,
                };
                write_audit(entry);
            }
        }
    }
}

/// Field visitor for extracting span/event fields
struct FieldVisitor<'a>(&'a mut HashMap<String, serde_json::Value>);

impl<'a> tracing::field::Visit for FieldVisitor<'a> {
    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.0.insert(field.name().to_string(), json!(value));
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.0.insert(field.name().to_string(), json!(value));
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.0.insert(field.name().to_string(), json!(value));
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.0.insert(field.name().to_string(), json!(value));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.0.insert(field.name().to_string(), json!(value));
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0
            .insert(field.name().to_string(), json!(format!("{:?}", value)));
    }
}
