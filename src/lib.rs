use pyo3::prelude::*;
use pyo3_log::{Logger, Caching};

mod exceptions;
use exceptions::{
    ConsumedSourceError, CouldNotConnectToRTPError, CouldNotOpenFileError, FfmpegError,
    SongbirdError, TrackError, UseAsyncConstructorError, YtdlError,
};

mod config;
mod driver;
mod event;
mod source;
mod seekable;
mod track;
mod track_handle;
mod utils;

/// The Songbird Python/Rust bindings
/// This module is written in Rust 🚀
#[pymodule]
fn songbird(py: Python, m: &PyModule) -> PyResult<()> {
    let _ = Logger::new(py, Caching::LoggersAndLevels)?.install();

    m.add_class::<driver::PyDriver>()?;
    m.add_class::<source::PySource>()?;
    m.add_class::<seekable::PyCompressedSource>()?;

    // Config
    m.add_class::<config::PyConfig>()?;
    m.add_class::<config::PyCryptoMode>()?;
    m.add_class::<config::PyDecodeMode>()?;
    m.add_class::<config::PyStrategy>()?;
    m.add_class::<config::PyBitrate>()?;

    // Track_handler
    m.add_class::<track_handle::PyPlayMode>()?;
    m.add_class::<track_handle::PyTrackHandle>()?;
    m.add_class::<track_handle::PyTrackState>()?;
    m.add_class::<track_handle::PyLoopState>()?;
    m.add_class::<track_handle::PyMetadata>()?;
    m.add_class::<track_handle::PyTrackHandle>()?;

    // Track
    m.add_class::<track::PyTrack>()?;
    track::register(py, m)?;

    // Events
    m.add_class::<event::PyEvent>()?;
    m.add_class::<event::PySpeakingState>()?;
    m.add_class::<event::PySpeaking>()?;
    m.add_class::<event::PySpeakingUpdateData>()?;
    m.add_class::<event::PyClientConnect>()?;
    m.add_class::<event::PyConnectData>()?;
    m.add_class::<event::PyDisconnectKind>()?;
    m.add_class::<event::PyDisconnectReason>()?;
    m.add_class::<event::PyVoiceData>()?;
    m.add_class::<event::PyRtp>()?;
    m.add_class::<event::PyRtpType>()?;

    m.add("ConsumedSourceError", py.get_type::<ConsumedSourceError>())?;
    m.add(
        "CouldNotConnectToRTPError",
        py.get_type::<CouldNotConnectToRTPError>(),
    )?;
    m.add(
        "CouldNotOpenFileError",
        py.get_type::<CouldNotOpenFileError>(),
    )?;
    m.add("FfmpegError", py.get_type::<FfmpegError>())?;
    m.add("SongbirdError", py.get_type::<SongbirdError>())?;
    m.add("TrackError", py.get_type::<TrackError>())?;
    m.add(
        "UseAsyncConstructorError",
        py.get_type::<UseAsyncConstructorError>(),
    )?;
    m.add("YtdlError", py.get_type::<YtdlError>())?;

    Ok(())
}
