use std::mem;
use std::sync::Arc;
use std::time::Duration;

use pyo3::prelude::*;
use songbird::tracks::{Track, TrackHandle};
use tokio::sync::Mutex;

use crate::source::PySource;
use crate::track_handle::{
    handle_track_result, PyLoopState, PyPlayMode, PyTrackHandle, PyTrackState,
};

/// Creates a ``Track`` and ``TrackHandle`` object. The track is used to play the ``Track`` and the TrackHandle
/// can be used to control it after it starts playing.
#[pyfunction]
#[pyo3(name = "create_player")]
pub fn py_create_player<'p>(py: Python<'p>, source: &mut PySource) -> PyResult<&'p PyAny> {
    source.raise_if_consumed()?;

    let source = source.source.clone();

    pyo3_asyncio::tokio::future_into_py(py, async move {
        let mut pyinput = source.lock().await;
        let old = mem::take(&mut *pyinput);

        let (track, handle) = songbird::create_player(old.unwrap());

        Ok((
            PyTrack {
                track: Arc::from(Mutex::from(Some(track))),
                handle: handle.clone(),
            },
            PyTrackHandle::from(handle),
        ))
    })
}

#[allow(unused_variables)]
pub(crate) fn register(py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(py_create_player, m)?)?;
    Ok(())
}

/// A Track. This is similar to a `Source` but you can control audio before its played.
/// This object should only be created through the ``create_player`` method.
#[pyclass(name = "Track")]
pub struct PyTrack {
    pub track: Arc<Mutex<Option<Track>>>,
    pub handle: TrackHandle,
}

#[pymethods(name = "Track")]
impl PyTrack {
    // Play the track.
    #[pyo3(text_signature = "($self)")]
    fn play<'p>(&'p mut self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let track = self.track.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            track.lock().await.as_mut().unwrap().play();
            Ok(())
        })
    }
    // Pause the track.
    #[pyo3(text_signature = "($self)")]
    fn pause<'p>(&'p mut self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let track = self.track.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            track.lock().await.as_mut().unwrap().pause();
            Ok(())
        })
    }

    ///Manually stops a track.
    ///Stopped/ended tracks cannot be restarted.
    #[pyo3(text_signature = "($self)")]
    fn stop<'p>(&'p mut self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let track = self.track.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            track.lock().await.as_mut().unwrap().stop();
            Ok(())
        })
    }

    /// Returns :data:`True` if the track is playing.
    #[pyo3(text_signature = "($self)")]
    fn playing<'p>(&'p mut self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let track = self.track.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            let play_mode = track.lock().await.as_mut().unwrap().playing();
            Ok(PyPlayMode::from(play_mode))
        })
    }
    /// Returns the volume of the track.
    #[pyo3(text_signature = "($self)")]
    fn volume<'p>(&'p mut self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let track = self.track.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            Ok(track.lock().await.as_mut().unwrap().volume())
        })
    }
    /// Sets the volume of the track.
    #[pyo3(text_signature = "($self, volume)")]
    fn set_volume<'p>(&'p mut self, py: Python<'p>, volume: f32) -> PyResult<&'p PyAny> {
        let track = self.track.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            track.lock().await.as_mut().unwrap().set_volume(volume);
            Ok(())
        })
    }
    /// Returns the position of the track.
    #[pyo3(text_signature = "($self)")]
    fn position<'p>(&mut self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let track = self.track.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            Ok(track
                .lock()
                .await
                .as_mut()
                .unwrap()
                .position()
                .as_secs_f64())
        })
    }
    /// Returns how long the track has been playing for.
    #[pyo3(text_signature = "($self)")]
    fn play_time<'p>(&mut self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let track = self.track.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            Ok(track
                .lock()
                .await
                .as_mut()
                .unwrap()
                .play_time()
                .as_secs_f64())
        })
    }
    /// Sets the loop count. If `loops` is None, it will loop forever.
    #[pyo3(text_signature = "($self, loops)")]
    fn set_loop_count<'p>(&mut self, py: Python<'p>, loops: Option<usize>) -> PyResult<&'p PyAny> {
        let track = self.track.clone();
        let loops = loops.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            handle_track_result(
                track
                    .lock()
                    .await
                    .as_mut()
                    .unwrap()
                    .set_loops(PyLoopState::from_usize(loops).as_songbird_loop_state()),
            )
        })
    }
    /// Ready a track for playing if it is lazily initialised.
    /// This won't matter until ``Restartable`` is implemented.
    #[pyo3(text_signature = "($self)")]
    fn make_playable<'p>(&mut self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let track = self.track.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            track.lock().await.as_mut().unwrap().make_playable();
            Ok(())
        })
    }
    /// Returns a copy of the track's state.
    #[pyo3(text_signature = "($self)")]
    fn state<'p>(&mut self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let track = self.track.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            Ok(PyTrackState::from(
                track.lock().await.as_mut().unwrap().state(),
            ))
        })
    }
    /// Seek to a specific point in the track.
    #[pyo3(text_signature = "($self)")]
    fn seek_time<'p>(&mut self, py: Python<'p>, position: f64) -> PyResult<&'p PyAny> {
        let track = self.track.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            match handle_track_result(
                track
                    .lock()
                    .await
                    .as_mut()
                    .unwrap()
                    .seek_time(Duration::from_secs_f64(position)),
            ) {
                Ok(dur) => Ok(dur.as_secs_f64()),
                Err(err) => Err(err),
            }
        })
    }
    ///Returns the track's UUID.
    #[pyo3(text_signature = "($self)")]
    fn uuid<'p>(&'p mut self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let track = self.track.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            Ok(track.lock().await.as_mut().unwrap().uuid().to_string())
        })
    }
}
