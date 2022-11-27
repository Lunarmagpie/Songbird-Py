use std::mem;
use std::sync::Arc;

use pyo3::prelude::*;
use songbird::driver::{Bitrate, Driver};
use songbird::id::{ChannelId, GuildId, UserId};
use songbird::Config;
use tokio::sync::Mutex;

use crate::config::PyConfig;
use crate::event::{EventHanlder, PyEvent};
use crate::exceptions::{CouldNotConnectToRTPError, UseAsyncConstructorError};
use crate::source::{PySource};
use crate::track::PyTrack;
use crate::track_handle::PyTrackHandle;

/// A connection to the Discord Voice gateway. The connection info must be from a
/// different library as Songbird doesn't provide a regular Gateway connection.
///
/// .. code-block:: python
///
///     async def main():
///         driver = await Driver.create()
///         await driver.connect(
///             token=token,
///             endpoint=endpoint,
///             session_id=session_id,
///             guild_id=guild_id,
///             channel_id=channel_id,
///             user_id=user_id
///         )
///     
///
/// See more examples in the `example` directory.
#[pyclass(name = "Driver")]
pub struct PyDriver {
    driver: Arc<Mutex<Driver>>,
}

#[pymethods]
impl PyDriver {
    /// This can not create a Driver so it is raises an exception.
    #[new]
    fn new() -> PyResult<Self> {
        Err(UseAsyncConstructorError::new_err(
            "`await Driver.create()` should be used to construct this class.",
        ))
    }

    /// Creates a driver for this class.
    /// Drivers must be created in an event loop so it has to be done like this.
    ///
    /// .. code-block:: python
    ///
    ///     from songbird import Driver
    ///     ...
    ///
    ///     driver = await Driver.create()
    ///
    #[staticmethod]
    #[args(config = "None")]
    #[pyo3(text_signature = "(config: Optional[Config]) -> 'None'")]
    fn create<'p>(py: Python<'p>, config: Option<&PyConfig>) -> PyResult<&'p PyAny> {
        let config: Config = match config {
            Some(py_config) => py_config.config.clone(),
            None => Config::default(),
        };

        pyo3_asyncio::tokio::future_into_py(py, async move {
            Ok(PyDriver {
                driver: Arc::new(Mutex::new(Driver::new(config))),
            })
        })
    }

    /// Connect to a voice channel
    /// 
    /// .. note:
    /// 
    ///     url can start with `wss://` or no protocol.
    ///
    /// Args:
    ///     token: Token recieved from the Discord gateway. This is not your bot token.
    ///     endpoint: Endpoint recieved from Discord gateway.
    ///     session_id: Session id recieved from Discord gateway.
    ///     guild_id: Guild id you want to connct to.
    ///     channel_id: Channel id you want to connect to.
    ///     user_id: User id of the current user.
    #[pyo3(
        text_signature = "($self, token: str, endpoint: str, session_id: str, guild_id: int, channel_id: int, user_id: int)"
    )]
    fn connect<'p>(
        &'p self,
        py: Python<'p>,
        token: String,
        endpoint: String,
        session_id: String,
        guild_id: u64,
        channel_id: u64,
        user_id: u64,
    ) -> PyResult<&'p PyAny> {
        let driver = self.driver.clone();

        let endpoint = endpoint.replace("wss://", "");

        pyo3_asyncio::tokio::future_into_py(py, async move {
            let res = driver
                .lock()
                .await
                .connect(songbird::ConnectionInfo {
                    channel_id: Some(ChannelId::from(channel_id)),
                    endpoint: endpoint,
                    guild_id: GuildId::from(guild_id),
                    session_id: session_id,
                    token: token,
                    user_id: UserId::from(user_id),
                })
                .await;

            match res {
                Err(err) => Err(CouldNotConnectToRTPError::new_err(format!("{:?}", err))),
                Ok(_) => Ok(()),
            }
        })
    }

    /// Disables the driver.
    /// This does not update your voice state to remove you from the voice channel.
    #[pyo3(text_signature = "()")]
    fn leave<'p>(&'p self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let driver = self.driver.clone();

        pyo3_asyncio::tokio::future_into_py(py, async move {
            driver.lock().await.leave();
            Ok(())
        })
    }

    /// Mutes the driver.
    #[pyo3(text_signature = "()")]
    fn mute<'p>(&'p self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let driver = self.driver.clone();

        pyo3_asyncio::tokio::future_into_py(py, async move {
            driver.lock().await.mute(true);
            Ok(())
        })
    }

    /// Unmutes the driver.
    #[pyo3(text_signature = "()")]
    fn unmute<'p>(&'p self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let driver = self.driver.clone();

        pyo3_asyncio::tokio::future_into_py(py, async move {
            driver.lock().await.mute(false);
            Ok(())
        })
    }

    /// Returns whether the driver is muted.
    #[pyo3(text_signature = "()")]
    fn is_muted<'p>(&'p self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let driver = self.driver.clone();

        pyo3_asyncio::tokio::future_into_py(py, async move { Ok(driver.lock().await.is_mute()) })
    }

    /// Plays a Playable object.
    /// Playable are activated when you try to play them. That means all errors are
    /// thrown in this method.
    /// 
    /// Raises
    /// ------
    /// ConsumedSourceError
    ///     Source was already played or used to create a track object.
    fn play_source<'p>(&'p self, py: Python<'p>, source: &mut PySource) -> PyResult<&'p PyAny> {
        source.raise_if_consumed()?;

        let driver = self.driver.clone();
        let source = source.source.clone();

        pyo3_asyncio::tokio::future_into_py(py, async move {
            let mut source = source.lock().await;
            let old = mem::take(&mut *source);

            let track_handle = driver.lock().await.play_source(old.unwrap());
            Ok(PyTrackHandle::from(track_handle))
        })
    }

    /// Same as `play_source` but stops all other sources from playing.
    /// 
    /// Raises
    /// ------
    /// ConsumedSourceError
    ///     Source was already played or used to create a track object.
    fn play_only_source<'p>(
        &'p self,
        py: Python<'p>,
        source: &mut PySource,
    ) -> PyResult<&'p PyAny> {
        source.raise_if_consumed()?;

        let driver = self.driver.clone();
        let source = source.source.clone();

        pyo3_asyncio::tokio::future_into_py(py, async move {
            let mut source = source.lock().await;
            let old = mem::take(&mut *source);

            let track_handle = driver.lock().await.play_only_source(old.unwrap());
            Ok(PyTrackHandle::from(track_handle))
        })
    }

    /// Plays a Track object. This makes the Track object unuseable.
    fn play<'p>(&'p self, py: Python<'p>, track: &PyTrack) -> PyResult<&'p PyAny> {
        let driver = self.driver.clone();
        let handle = PyTrackHandle::from(track.handle.clone());
        let track = track.track.clone();

        pyo3_asyncio::tokio::future_into_py(py, async move {
            let mut inner = track.lock().await;
            let old = mem::take(&mut *inner);
            driver.lock().await.play(old.unwrap());
            Ok(handle)
        })
    }

    /// Same as `play` but stops all other sources from playing.
    fn play_only<'p>(&'p self, py: Python<'p>, track: &PyTrack) -> PyResult<&'p PyAny> {
        let driver = self.driver.clone();
        let handle = PyTrackHandle::from(track.handle.clone());
        let track = track.track.clone();

        pyo3_asyncio::tokio::future_into_py(py, async move {
            let mut inner = track.lock().await;
            let old = mem::take(&mut *inner);
            driver.lock().await.play_only(old.unwrap());
            Ok(handle)
        })
    }

    /// Sets the bitrate to a i32
    fn set_bitrate<'p>(&'p self, py: Python<'p>, bitrate: i32) -> PyResult<&'p PyAny> {
        let driver = self.driver.clone();

        pyo3_asyncio::tokio::future_into_py(py, async move {
            Ok(driver
                .lock()
                .await
                .set_bitrate(Bitrate::BitsPerSecond(bitrate)))
        })
    }

    /// Sets the bitrate to a Bitrate::Max
    fn set_bitrate_to_max<'p>(&'p self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let driver = self.driver.clone();

        pyo3_asyncio::tokio::future_into_py(py, async move {
            Ok(driver.lock().await.set_bitrate(Bitrate::Max))
        })
    }

    /// Sets the bitrate to Bitrate::Auto
    fn set_bitrate_to_auto<'p>(&'p self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let driver = self.driver.clone();

        pyo3_asyncio::tokio::future_into_py(py, async move {
            Ok(driver.lock().await.set_bitrate(Bitrate::Auto))
        })
    }

    /// Stops playing audio from all sources.
    fn stop<'p>(&'p self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let driver = self.driver.clone();

        pyo3_asyncio::tokio::future_into_py(py, async move { Ok(driver.lock().await.stop()) })
    }

    /// Set the config for this Driver
    fn set_config<'p>(&'p self, py: Python<'p>, config: &PyConfig) -> PyResult<&'p PyAny> {
        let driver = self.driver.clone();
        let config = config.config.clone();

        pyo3_asyncio::tokio::future_into_py(py, async move {
            Ok(driver.lock().await.set_config(config))
        })
    }

    /// Returns a copy of the config for this Driver
    fn get_config<'p>(&'p self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let driver = self.driver.clone();

        pyo3_asyncio::tokio::future_into_py(py, async move {
            Ok(PyConfig {
                config: driver.lock().await.config().clone(),
            })
        })
    }

    fn add_event<'p>(
        &'p self,
        py: Python<'p>,
        event: PyEvent,
        call: PyObject,
    ) -> PyResult<&'p PyAny> {
        let driver = self.driver.clone();

        let event_loop = pyo3_asyncio::get_running_loop(py)?;
        let handler = EventHanlder {
            coro: call,
            event_loop: event_loop.into_py(py),
        };

        pyo3_asyncio::tokio::future_into_py(py, async move {
            Ok(driver.lock().await.add_global_event(event.event, handler))
        })
    }

    fn remove_all_events<'p>(&'p self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let driver = self.driver.clone();

        pyo3_asyncio::tokio::future_into_py(py, async move {
            Ok(driver.lock().await.remove_all_global_events())
        })
    }
}
