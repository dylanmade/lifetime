use lifetime_core::model::ObservationKind;

/// Captures the current observable state of the host device.
///
/// A single call to [`Sampler::sample`] may return zero or more observation
/// kinds — e.g. a desktop sampler typically yields one `AppUsage` and one
/// `Idle` per tick. Implementations should be cheap to call (single-digit ms)
/// since they're invoked on a polling cadence.
pub trait Sampler: Send + Sync {
    fn sample(&self) -> Vec<ObservationKind>;
}
