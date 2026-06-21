mod cce;
mod errors;
mod similarity;
mod store;
mod subscriptions;
mod types;

use pyo3::prelude::*;

use cce::{cce_encode_bytes_raw, cce_encode_f64_bits, cce_encode_value, compute_event_id};
use errors::register as register_errors;
use similarity::PyHnswProvider;
use store::{PyCausationIter, PyCorrelationIter, PyRangeIter, PyStore};
use subscriptions::PyRawSubscriptionHandle;
use types::{
    PyAggregateQuery, PyAppend, PyBranchInfo, PyBranchSegment, PyCreateBranch, PyEventId,
    PyOpenOptions, PyReadOutcome, PyReadQuery, PySamplingMode, PySnapshotInfo, PyStoredEvent,
    PyStreamInfo, PySubscriptionMode, PyTruncationCursor,
};

#[pymodule]
fn _fossic(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Classes
    m.add_class::<PyStore>()?;
    m.add_class::<PyHnswProvider>()?;
    m.add_class::<PyEventId>()?;
    m.add_class::<PyStoredEvent>()?;
    m.add_class::<PyAppend>()?;
    m.add_class::<PyReadQuery>()?;
    m.add_class::<PyOpenOptions>()?;
    m.add_class::<PyStreamInfo>()?;
    m.add_class::<PyBranchInfo>()?;
    m.add_class::<PyBranchSegment>()?;
    m.add_class::<PyCreateBranch>()?;
    m.add_class::<PySnapshotInfo>()?;
    m.add_class::<PySubscriptionMode>()?;
    m.add_class::<PyAggregateQuery>()?;
    m.add_class::<PyRawSubscriptionHandle>()?;
    m.add_class::<PyTruncationCursor>()?;
    m.add_class::<PySamplingMode>()?;
    m.add_class::<PyReadOutcome>()?;
    m.add_class::<PyRangeIter>()?;
    m.add_class::<PyCorrelationIter>()?;
    m.add_class::<PyCausationIter>()?;

    // Exception hierarchy
    register_errors(m)?;

    // CCE encoding (testing / tooling; rarely needed in production code)
    m.add_function(wrap_pyfunction!(cce_encode_value, m)?)?;
    m.add_function(wrap_pyfunction!(cce_encode_bytes_raw, m)?)?;
    m.add_function(wrap_pyfunction!(cce_encode_f64_bits, m)?)?;
    m.add_function(wrap_pyfunction!(compute_event_id, m)?)?;

    Ok(())
}
