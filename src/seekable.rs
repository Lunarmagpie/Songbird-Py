use std::mem;

use pyo3::prelude::*;
use songbird::input::{cached::Compressed};

use crate::config::PyBitrate;
use crate::exceptions::{
    ConsumedSourceError, CouldNotConstructError, UseAsyncConstructorError,
};
use crate::source::PySource;

#[pyclass(name = "CompressedSource")]
pub struct PyCompressedSource {
    compressed: Option<Compressed>,
}

impl From<Compressed> for PyCompressedSource {
    fn from(memory: Compressed) -> Self {
        Self {
            compressed: Some(memory),
        }
    }
}

#[pymethods]
impl PyCompressedSource {
    #[new]
    fn new() -> PyResult<Self> {
        Err(UseAsyncConstructorError::new_err(
            "Use `CompressedSource.from_source` to create a `CompressedSource` object.",
        ))
    }

    /// Convert the `CompressedSource` into a `Source`
    fn into_source(&mut self) -> Result<PySource, PyErr> {
        let maybe_compressed = mem::take(&mut self.compressed);
        if let Some(compressed) = maybe_compressed {
            Ok(PySource::from(compressed.into()))
        } else {
            Err(ConsumedSourceError::new_err(
                "CompressedSource already converted to source.",
            ))
        }
    }

    /// Create a `CompressedSource` from a `Source`.
    ///
    /// .. code-block:: python
    ///
    ///     from songbird import CompressedSource, ytdl
    ///     compressed = CompressedSource.from_source(await ytdl("https://www.youtube.com/watch?v=r25MAkzkTF4"))
    ///
    #[staticmethod]
    fn from_source<'p>(
        py: Python<'p>,
        input: &PySource,
        bitrate: &PyBitrate,
    ) -> PyResult<&'p PyAny> {
        let source = input.source.clone();
        let bitrate = bitrate.bitrate;

        pyo3_asyncio::tokio::future_into_py(py, async move {
            let mut source = source.lock().await;
            let old = mem::take(&mut *source);

            match Compressed::new(old.unwrap(), bitrate) {
                Ok(c) => Ok(Self {
                    compressed: Some(c),
                }),
                Err(reason) => Err(CouldNotConstructError::new_err(reason.to_string())),
            }
        })
    }
}
