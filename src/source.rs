use std::fs::File;
use std::sync::Arc;

use tokio::sync::Mutex;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString};
use songbird::input::{Input, Reader};

use crate::exceptions::{ConsumedSourceError, CouldNotOpenFileError, FfmpegError, YtdlError};
use crate::track_handle::PyMetadata;

mod builtins {
    pyo3::import_exception!(builtins, FileNotFoundError);
}

#[pyclass(name = "Source")]
pub struct PySource {
    /// Represents an object that can be turned into an input.
    /// Inputs are buffered in a Playable object due to Inputs not being thread safe.
    /// This method of creating inputs allows you to use an Input multiple times in
    /// Python, which is probably expected.
    pub source: Arc<Mutex<Option<Input>>>,
    consumed: bool,
}

impl PySource {
    pub fn from(input: Input) -> Self {
        Self {
            source: Arc::from(Mutex::from(Some(input))),
            consumed: false,
        }
    }

    pub fn raise_if_consumed(&mut self) -> Result<(), PyErr> {
        if self.consumed {
            Err(ConsumedSourceError::new_err(concat!(
                "Source object has already been used! Sources can only create a track",
                " or be played in a driver once."
            )))
        } else {
            self.mark_consumed();
            Ok(())
        }
    }

    pub fn mark_consumed(&mut self) -> () {
        self.consumed = true
    }
}

fn map_args<'a>(value: Option<&'a PyAny>) -> Result<Vec<String>, PyErr> {
    if value.is_none() {
        return Ok(vec![]);
    };
    let items = value.unwrap().downcast::<PyString>()?;
    let items = items.to_string();

    Ok(items
        .split_whitespace()
        .map(|x| x.to_string())
        .collect::<Vec<String>>())
}

#[pymethods]
impl PySource {
    /// Use youtube dl to play a video from a URL
    ///
    /// Example
    ///
    /// .. code-block:: python
    ///
    ///     await driver.play(Source.ytdl("https://www.youtube.com/watch?v=n5n7CSGPzqw"))
    #[staticmethod]
    fn ytdl<'p>(py: Python, url: String) -> PyResult<&PyAny> {
        pyo3_asyncio::tokio::future_into_py(py, async move {
            match songbird::ytdl(url).await {
                Ok(res) => Ok(Self::from(res)),
                Err(err) => Err(YtdlError::new_err(format!("{:?}", err))),
            }
        })
    }

    /// Function used to play most audio formats
    ///
    /// .. code-block:: python
    ///
    ///     await driver.play(Source.ffmpeg("song.mp3"))
    #[staticmethod]
    #[args(kwargs = "**")]
    fn ffmpeg<'a, 'p>(
        py: Python<'p>,
        filepath: String,
        kwargs: Option<&'a PyDict>,
    ) -> PyResult<&'p PyAny> {
        let pre_input_args: Vec<String>;
        let args: Vec<String>;

        if let Some(kwargs) = kwargs {
            let _pre_input_args = kwargs.get_item("pre_input_args");
            let _args = kwargs.get_item("args");

            pre_input_args = map_args(_pre_input_args)?;
            args = map_args(_args)?;
        } else {
            pre_input_args = vec![];
            args = vec![];
        };

        pyo3_asyncio::tokio::future_into_py(py, async move {
            let pre_input_args: Vec<&str> = pre_input_args.iter().map(String::as_str).collect();
            let args: Vec<&str> = args.iter().map(String::as_str).collect();

            if !std::path::Path::new(&filepath).exists() {
                return Err(builtins::FileNotFoundError::new_err(format!(
                    "File `{}` does not exist",
                    filepath
                )));
            };

            match if pre_input_args.is_empty() && args.is_empty() {
                songbird::ffmpeg(filepath).await
            } else {
                songbird::input::ffmpeg_optioned(filepath, pre_input_args.as_ref(), args.as_ref())
                    .await
            } {
                Ok(res) => Ok(Self::from(res)),
                Err(err) => Err(FfmpegError::new_err(format!("{:?}", err))),
            }
        })
    }

    /// Returns the Metadata for this source
    fn metadata<'p>(&'p self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let source = self.source.clone();

        pyo3_asyncio::tokio::future_into_py(py, async move {
            let guard = source.lock().await;
            Ok(PyMetadata::from(&guard.as_ref().unwrap().metadata))
        })
    }

    /// Returns whether the souce is stereo
    fn stereo<'p>(&'p self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let source = self.source.clone();

        pyo3_asyncio::tokio::future_into_py(py, async move {
            Ok(source.lock().await.as_ref().unwrap().stereo)
        })
    }
}
