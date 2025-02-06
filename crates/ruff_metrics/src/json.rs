use std::cell::RefCell;
use std::fmt::Write as _;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use metrics::{
    Counter, CounterFn, Gauge, GaugeFn, Histogram, HistogramFn, Key, KeyName, Metadata, Recorder,
    SharedString, Unit,
};
use metrics_util::registry::{Registry, Storage};
use serde_json::value::Map;
use serde_json::Value;

/// A [metrics] recorder that outputs metrics in a simple JSON format, typically to a file for
/// later analysis. We do not buffer the metrics.
///
/// Each output record will include a `key` field with the name of the metric. Any labels will also
/// appear as additional JSON fields.
///
/// Counters and gauges will include `delta` and `value` fields, providing the amount that the
/// counter changed by, and the resulting total value.
///
/// Histograms will include `value` and `count` fields. We do not aggregate histogram data in any
/// way.
pub struct JsonRecorder {
    registry: Registry<Key, PrerenderedAtomicStorage>,
}

impl JsonRecorder {
    /// Creates a new `JsonRecorder` that will output JSON metrics to a destination that implements
    /// [`std::io::Write`].
    pub fn new<D>(dest: D) -> JsonRecorder
    where
        D: Write + Send + 'static,
    {
        let dest = Arc::new(Mutex::new(dest));
        let storage = PrerenderedAtomicStorage { dest };
        let registry = Registry::new(storage);
        JsonRecorder { registry }
    }
}

impl Recorder for JsonRecorder {
    // We currently ignore metrics descriptions.
    fn describe_counter(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}
    fn describe_gauge(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}
    fn describe_histogram(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn register_counter(&self, key: &Key, _metadata: &Metadata<'_>) -> Counter {
        self.registry
            .get_or_create_counter(key, |existing| Counter::from_arc(Arc::clone(existing)))
    }

    fn register_gauge(&self, key: &Key, _metadata: &Metadata<'_>) -> Gauge {
        self.registry
            .get_or_create_gauge(key, |existing| Gauge::from_arc(Arc::clone(existing)))
    }

    fn register_histogram(&self, key: &Key, _metadata: &Metadata<'_>) -> Histogram {
        self.registry
            .get_or_create_histogram(key, |existing| Histogram::from_arc(Arc::clone(existing)))
    }
}

struct PrerenderedAtomicStorage {
    dest: Arc<Mutex<dyn Write + Send>>,
}

impl Storage<Key> for PrerenderedAtomicStorage {
    type Counter = Arc<Metric>;
    type Gauge = Arc<Metric>;
    type Histogram = Arc<Metric>;

    fn counter(&self, key: &Key) -> Self::Counter {
        Arc::new(Metric::new(key, self.dest.clone()))
    }

    fn gauge(&self, key: &Key) -> Self::Gauge {
        Arc::new(Metric::new(key, self.dest.clone()))
    }

    fn histogram(&self, key: &Key) -> Self::Histogram {
        Arc::new(Metric::new(key, self.dest.clone()))
    }
}

struct Metric {
    /// The metric key's name and labels, rendered into JSON on a single line, with the trailing
    /// `}` removed. (This makes it easy to append the JSON rendering of each data point without
    /// having to re-render the information about the metrics key.)
    name_and_labels: String,
    dest: Arc<Mutex<dyn Write + Send>>,
    value: AtomicU64,
}

impl Metric {
    fn new(key: &Key, dest: Arc<Mutex<dyn Write + Send>>) -> Metric {
        let mut json = Map::default();
        json.insert("key".to_string(), key.name().into());
        for label in key.labels() {
            json.insert(label.key().to_string(), label.value().into());
        }
        let mut name_and_labels = serde_json::to_string(&Value::Object(json))
            .expect("should always be able to render JSON object containing only strings");
        // Trim the trailing '}'
        let _ = name_and_labels.pop();
        Metric {
            name_and_labels,
            dest,
            value: AtomicU64::default(),
        }
    }

    fn output<F>(&self, f: F)
    where
        F: FnOnce(&mut String),
    {
        // Render into a thread-local String buffer, and then output the resulting line in a single
        // call. This ensures that the output from multiple threads does not get intermingled.
        thread_local! {
            static BUFFERS: RefCell<String> = RefCell::new(String::new());
        }
        BUFFERS.with(|buffer| {
            let mut buffer = buffer.borrow_mut();
            buffer.clear();
            buffer.push_str(&self.name_and_labels);
            if let Ok(timestamp) = SystemTime::now().duration_since(UNIX_EPOCH) {
                write!(&mut buffer, ",\"timestamp\":{}", timestamp.as_secs_f64()).unwrap();
            }
            f(&mut buffer);
            buffer.push_str("}\n");
            let _ = self.dest.lock().unwrap().write(buffer.as_bytes());
        })
    }
}

impl CounterFn for Metric {
    fn increment(&self, delta: u64) {
        let old_value = self.value.fetch_add(delta, Ordering::Relaxed);
        let new_value = old_value + delta;
        self.output(|buffer| {
            write!(buffer, ",\"delta\":{delta},\"value\":{new_value}").unwrap();
        });
    }

    fn absolute(&self, value: u64) {
        self.value.store(value, Ordering::Relaxed);
        self.output(|buffer| {
            write!(buffer, ",\"value\":{value}").unwrap();
        });
    }
}

impl GaugeFn for Metric {
    fn increment(&self, delta: f64) {
        let mut new_value: f64 = 0.0;
        self.value
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old_value| {
                new_value = f64::from_bits(old_value) + delta;
                Some(f64::to_bits(new_value))
            })
            .expect("should never fail to update gauge");
        self.output(|buffer| {
            write!(buffer, ",\"delta\":{delta},\"value\":{new_value}").unwrap();
        });
    }

    fn decrement(&self, delta: f64) {
        let mut new_value: f64 = 0.0;
        self.value
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old_value| {
                new_value = f64::from_bits(old_value) - delta;
                Some(f64::to_bits(new_value))
            })
            .expect("should never fail to update gauge");
        self.output(|buffer| {
            write!(buffer, ",\"delta\":{delta},\"value\":{new_value}").unwrap();
        });
    }

    fn set(&self, value: f64) {
        self.value.store(value.to_bits(), Ordering::Relaxed);
        self.output(|buffer| {
            write!(buffer, ",\"value\":{value}").unwrap();
        });
    }
}

impl HistogramFn for Metric {
    fn record(&self, value: f64) {
        self.record_many(value, 1);
    }

    fn record_many(&self, value: f64, count: usize) {
        self.output(|buffer| {
            write!(buffer, ",\"value\":{value},\"count\":{count}").unwrap();
        });
    }
}
