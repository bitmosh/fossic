mod errors;
mod store;
mod subscriptions;
mod types;

use pyo3::prelude::*;

use errors::register as register_errors;
use store::PyStore;
use subscriptions::PyRawSubscriptionHandle;
use types::{
    PyAggregateQuery, PyAppend, PyBranchInfo, PyBranchSegment, PyCreateBranch, PyEventId,
    PyOpenOptions, PyReadQuery, PySnapshotInfo, PyStoredEvent, PyStreamInfo, PySubscriptionMode,
};

#[pymodule]
fn _fossic(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Classes
    m.add_class::<PyStore>()?;
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

    // Exception hierarchy
    register_errors(m)?;

    Ok(())
}
