use fossic::Error as FossicError;
use pyo3::prelude::*;

// ── Exception hierarchy ───────────────────────────────────────────────────────

pyo3::create_exception!(fossic, FossicBaseError, pyo3::exceptions::PyException);

pyo3::create_exception!(fossic, StreamNotDeclaredError, FossicBaseError);
pyo3::create_exception!(fossic, InvalidStreamIdError, FossicBaseError);
pyo3::create_exception!(fossic, InvalidEventIdError, FossicBaseError);
pyo3::create_exception!(fossic, StoreNotFoundError, FossicBaseError);
pyo3::create_exception!(fossic, SchemaMismatchError, FossicBaseError);
pyo3::create_exception!(fossic, NotImplementedError, FossicBaseError);
pyo3::create_exception!(fossic, BranchNotFoundError, FossicBaseError);
pyo3::create_exception!(fossic, BranchLifecycleError, FossicBaseError);
pyo3::create_exception!(fossic, InvalidBranchIdError, FossicBaseError);
pyo3::create_exception!(fossic, ReducerPatternAmbiguousError, FossicBaseError);
pyo3::create_exception!(fossic, ReducerNotFoundError, FossicBaseError);
pyo3::create_exception!(fossic, ReducerNotFoundByNameError, FossicBaseError);
pyo3::create_exception!(fossic, ReducerCallError, FossicBaseError);
pyo3::create_exception!(fossic, NoEventsToSnapshotError, FossicBaseError);
pyo3::create_exception!(fossic, PurgeConfirmationError, FossicBaseError);
pyo3::create_exception!(fossic, EventNotFoundError, FossicBaseError);
pyo3::create_exception!(fossic, UpcasterChainGapError, FossicBaseError);
pyo3::create_exception!(fossic, StorageError, FossicBaseError);

/// Convert a fossic Rust error to the appropriate Python exception.
pub fn to_py_err(e: FossicError) -> PyErr {
    let msg = e.to_string();
    match e {
        FossicError::StreamNotDeclared { .. } => StreamNotDeclaredError::new_err(msg),
        FossicError::InvalidStreamId { .. } => InvalidStreamIdError::new_err(msg),
        FossicError::InvalidEventId(_) => InvalidEventIdError::new_err(msg),
        FossicError::StoreNotFound { .. } => StoreNotFoundError::new_err(msg),
        FossicError::SchemaMismatch { .. } => SchemaMismatchError::new_err(msg),
        FossicError::NotImplemented { .. } => NotImplementedError::new_err(msg),
        FossicError::BranchNotFound { .. } => BranchNotFoundError::new_err(msg),
        FossicError::BranchLifecycleError { .. } => BranchLifecycleError::new_err(msg),
        FossicError::InvalidBranchId { .. } => InvalidBranchIdError::new_err(msg),
        FossicError::ReducerPatternAmbiguous { .. } => ReducerPatternAmbiguousError::new_err(msg),
        FossicError::ReducerNotFound { .. } => ReducerNotFoundError::new_err(msg),
        FossicError::ReducerNotFoundByName { .. } => ReducerNotFoundByNameError::new_err(msg),
        FossicError::ReducerError { .. } => ReducerCallError::new_err(msg),
        FossicError::NoEventsToSnapshot { .. } => NoEventsToSnapshotError::new_err(msg),
        FossicError::PurgeConfirmationError { .. } => PurgeConfirmationError::new_err(msg),
        FossicError::EventNotFound { .. } => EventNotFoundError::new_err(msg),
        FossicError::UpcasterChainGap { .. } => UpcasterChainGapError::new_err(msg),
        _ => StorageError::new_err(msg),
    }
}

/// Register all exception types on the Python module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("FossicError", m.py().get_type::<FossicBaseError>())?;
    m.add(
        "StreamNotDeclaredError",
        m.py().get_type::<StreamNotDeclaredError>(),
    )?;
    m.add(
        "InvalidStreamIdError",
        m.py().get_type::<InvalidStreamIdError>(),
    )?;
    m.add(
        "InvalidEventIdError",
        m.py().get_type::<InvalidEventIdError>(),
    )?;
    m.add(
        "StoreNotFoundError",
        m.py().get_type::<StoreNotFoundError>(),
    )?;
    m.add(
        "SchemaMismatchError",
        m.py().get_type::<SchemaMismatchError>(),
    )?;
    m.add(
        "NotImplementedError",
        m.py().get_type::<NotImplementedError>(),
    )?;
    m.add(
        "BranchNotFoundError",
        m.py().get_type::<BranchNotFoundError>(),
    )?;
    m.add(
        "BranchLifecycleError",
        m.py().get_type::<BranchLifecycleError>(),
    )?;
    m.add(
        "InvalidBranchIdError",
        m.py().get_type::<InvalidBranchIdError>(),
    )?;
    m.add(
        "ReducerPatternAmbiguousError",
        m.py().get_type::<ReducerPatternAmbiguousError>(),
    )?;
    m.add(
        "ReducerNotFoundError",
        m.py().get_type::<ReducerNotFoundError>(),
    )?;
    m.add(
        "ReducerNotFoundByNameError",
        m.py().get_type::<ReducerNotFoundByNameError>(),
    )?;
    m.add("ReducerCallError", m.py().get_type::<ReducerCallError>())?;
    m.add(
        "NoEventsToSnapshotError",
        m.py().get_type::<NoEventsToSnapshotError>(),
    )?;
    m.add(
        "PurgeConfirmationError",
        m.py().get_type::<PurgeConfirmationError>(),
    )?;
    m.add(
        "EventNotFoundError",
        m.py().get_type::<EventNotFoundError>(),
    )?;
    m.add(
        "UpcasterChainGapError",
        m.py().get_type::<UpcasterChainGapError>(),
    )?;
    m.add("StorageError", m.py().get_type::<StorageError>())?;
    Ok(())
}
