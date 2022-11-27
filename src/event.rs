use std::time::Duration;

use async_trait::async_trait;
use pyo3::basic::CompareOp;
use pyo3::exceptions::{PyNotImplementedError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use songbird::events::context_data::{ConnectData, DisconnectKind, DisconnectReason, VoiceData};
use songbird::model::SpeakingState;
use songbird::{CoreEvent, Event, EventContext, EventHandler, TrackEvent};

use crate::track_handle::{PyTrackHandle, PyTrackState};
use crate::utils::unwrap_f64_to_duration;

use discortp::rtp::{Rtp, RtpType};

#[derive(Clone)]
pub struct EventHanlder {
    pub coro: PyObject,
    pub event_loop: PyObject,
}

impl EventHanlder {
    pub fn new(coro: PyObject, event_loop: PyObject) -> Self {
        Self { coro, event_loop }
    }

    fn call_event(&self, py: Python, ctx: &EventContext) -> Result<PyObject, PyErr> {
        let asyncio = py.import("asyncio")?;
        let ensure_future = asyncio.getattr("ensure_future")?;

        let coro_wrapper: Py<PyAny> = PyModule::from_code(
            py,
            "def wrap(coro, args):
                if not isinstance(args, tuple):
                    args = (args,)
                return coro(*args)
            ",
            "",
            "",
        )?
        .getattr("wrap")?
        .into();

        let coro = coro_wrapper.call1(py, (&self.coro, event_to_py(py, ctx)?))?;

        let kwargs = PyDict::from_sequence(py, [("loop", &self.event_loop)].into_py(py))?;

        let res = ensure_future.call((coro,), Some(kwargs))?;

        Ok(PyObject::from(res))
    }
}

#[async_trait]
impl EventHandler for EventHanlder {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        Python::with_gil(|py| match self.call_event(py, ctx) {
            Ok(_) => None,
            Err(e) => {
                e.print_and_set_sys_last_vars(py);
                None
            }
        })
    }
}

fn event_to_py(py: Python, event: &EventContext) -> PyResult<PyObject> {
    match event {
        EventContext::Track(track_array) => Ok((
            PyTrackState::from(*track_array[0].0),
            PyTrackHandle::from(track_array[0].1.clone()),
        )
            .into_py(py)),
        EventContext::SpeakingStateUpdate(speaking) => Ok(PySpeaking {
            delay: speaking.delay,
            speaking: PySpeakingState::from(speaking.speaking),
            ssrc: speaking.ssrc,
            user_id: match speaking.user_id {
                Some(id) => Some(id.0),
                None => None,
            },
        }
        .into_py(py)),
        EventContext::VoicePacket(data) => Ok(PyVoiceData::from(data).into_py(py)),
        EventContext::SpeakingUpdate(data) => Ok(PySpeakingUpdateData {
            speaking: data.speaking,
            ssrc: data.ssrc,
        }
        .into_py(py)),
        EventContext::ClientDisconnect(disconnect) => Ok(disconnect.user_id.0.into_py(py)),
        EventContext::DriverConnect(connect) => Ok(PyConnectData::from(connect).into_py(py)),
        EventContext::DriverReconnect(connect) => Ok(PyConnectData::from(connect).into_py(py)),
        EventContext::DriverDisconnect(disconnect) => Ok(PyDisconnectData {
            kind: PyDisconnectKind::from(disconnect.kind),
            reason: match disconnect.reason {
                Some(reason) => Some(PyDisconnectReason::from(reason)),
                None => None,
            },
            channel_id: match disconnect.channel_id {
                Some(id) => Some(id.0.into()),
                None => None,
            },
            guild_id: disconnect.guild_id.0.into(),
            session_id: disconnect.session_id.to_string(),
        }
        .into_py(py)),
        _ => Err(PyNotImplementedError::new_err(format!(
            "{:?} is not implemented or deprecated",
            event
        ))),
    }
}

#[pyclass(name = "Event")]
#[derive(Clone)]
pub struct PyEvent {
    pub event: Event,
}

impl PyEvent {
    fn from(event: Event) -> Self {
        Self { event }
    }
}

#[allow(non_snake_case)]
#[pymethods]
impl PyEvent {
    #[staticmethod]
    #[args(phase = "None")]
    /// Repeats every duration.
    fn periodic(duration: f64, phase: Option<f64>) -> Self {
        Self::from(Event::Periodic(
            Duration::from_secs_f64(duration),
            unwrap_f64_to_duration(phase),
        ))
    }
    #[staticmethod]
    /// Event fired after the delay value.
    fn delayed(duration: f64) -> Self {
        Self::from(Event::Delayed(Duration::from_secs_f64(duration)))
    }

    #[staticmethod]
    fn Cancel() -> Self {
        Self::from(Event::Cancel)
    }

    // Track Events

    #[classattr]
    /// The attached track has resumed playing.
    /// This event will not fire when a track first starts, but will fire
    /// when a track changes from, e.g., paused to playing. This is most
    /// relevant for queue users.
    fn Play() -> Self {
        Self::from(Event::Track(TrackEvent::Play))
    }
    #[classattr]
    /// The track has been paused.
    fn Pause() -> Self {
        Self::from(Event::Track(TrackEvent::Pause))
    }
    #[classattr]
    /// The track ended.
    fn End() -> Self {
        Self::from(Event::Track(TrackEvent::End))
    }
    /// The track looped.
    #[classattr]
    fn Loop() -> Self {
        Self::from(Event::Track(TrackEvent::Loop))
    }

    // Core Events

    #[classattr]
    /// Fired on receipt of a speaking state update from another host.
    fn SpeakingStateUpdate() -> Self {
        Self::from(Event::Core(CoreEvent::SpeakingStateUpdate))
    }
    #[classattr]
    /// Fires when a source starts speaking, or stops speaking (i.e., 5 consecutive silent frames).
    fn SpeakingUpdate() -> Self {
        Self::from(Event::Core(CoreEvent::SpeakingUpdate))
    }
    #[classattr]
    /// Fires on receipt of a voice packet from another stream in the voice call.
    fn VoicePacket() -> Self {
        Self::from(Event::Core(CoreEvent::VoicePacket))
    }
    #[classattr]
    /// Fires on receipt of an RTCP packet, containing various call stats such as latency reports.
    fn RtcpPacket() -> Self {
        Self::from(Event::Core(CoreEvent::RtcpPacket))
    }
    #[classattr]
    /// Fires whenever a user disconnects from the same stream as the bot.
    fn ClientDisconnect() -> Self {
        Self::from(Event::Core(CoreEvent::ClientDisconnect))
    }
    #[classattr]
    /// Fires when this driver successfully connects to a voice channel.
    fn DriverConnect() -> Self {
        Self::from(Event::Core(CoreEvent::DriverConnect))
    }
    #[classattr]
    /// Fires when this driver successfully reconnects after a network error.
    fn DriverReconnect() -> Self {
        Self::from(Event::Core(CoreEvent::DriverReconnect))
    }
    /// Fires when this driver fails to connect to, or drops from, a voice channel.
    #[classattr]
    fn DriverDisconnect() -> Self {
        Self::from(Event::Core(CoreEvent::DriverDisconnect))
    }
}

#[pyclass(name = "SpeakingState")]
#[derive(Clone)]
pub struct PySpeakingState {
    pub speaking_state: SpeakingState,
}

impl PySpeakingState {
    fn from(speaking_state: SpeakingState) -> Self {
        Self { speaking_state }
    }
}

#[allow(non_snake_case)]
#[pymethods]
impl PySpeakingState {
    #[classattr]
    fn Microphone() -> Self {
        Self::from(SpeakingState::MICROPHONE)
    }
    #[classattr]
    fn Soundshare() -> Self {
        Self::from(SpeakingState::SOUNDSHARE)
    }
    #[classattr]
    fn Priority() -> Self {
        Self::from(SpeakingState::PRIORITY)
    }
}

#[pyclass(name = "Speaking")]
pub struct PySpeaking {
    #[pyo3(get)]
    delay: Option<u32>,
    #[pyo3(get)]
    speaking: PySpeakingState,
    #[pyo3(get)]
    ssrc: u32,
    #[pyo3(get)]
    user_id: Option<u64>,
}

#[pyclass(name = "RtpType")]
#[derive(Clone)]
pub struct PyRtpType {
    rtptype: RtpType,
}

/// The current state of the track. ie. Paused/Unpaused.
impl PyRtpType {
    pub fn from(rtptype: RtpType) -> Self {
        Self { rtptype }
    }
}

#[allow(non_snake_case)]
#[pymethods]
impl PyRtpType {
    #[classattr]
    fn Pcmu() -> Self {
        Self::from(RtpType::Pcmu)
    }
    #[classattr]
    fn Gsm() -> Self {
        Self::from(RtpType::Gsm)
    }
    #[classattr]
    fn G723() -> Self {
        Self::from(RtpType::G723)
    }
    #[classattr]
    fn Dvi4_1() -> Self {
        Self::from(RtpType::Dvi4(1))
    }
    #[classattr]
    fn Dvi4_2() -> Self {
        Self::from(RtpType::Dvi4(2))
    }
    #[classattr]
    fn Dvi4_3() -> Self {
        Self::from(RtpType::Dvi4(3))
    }
    #[classattr]
    fn Dvi4_4() -> Self {
        Self::from(RtpType::Dvi4(4))
    }
    #[classattr]
    fn Dvi4_5() -> Self {
        Self::from(RtpType::Dvi4(5))
    }
    #[classattr]
    fn Dvi4_6() -> Self {
        Self::from(RtpType::Dvi4(6))
    }
    #[classattr]
    fn Dvi4_7() -> Self {
        Self::from(RtpType::Dvi4(7))
    }
    #[classattr]
    fn Dvi4_8() -> Self {
        Self::from(RtpType::Dvi4(8))
    }
    #[classattr]
    fn Lpc() -> Self {
        Self::from(RtpType::Lpc)
    }
    #[classattr]
    fn Pcma() -> Self {
        Self::from(RtpType::Pcma)
    }
    #[classattr]
    fn G722() -> Self {
        Self::from(RtpType::G722)
    }
    #[classattr]
    fn L16Stereo() -> Self {
        Self::from(RtpType::L16Stereo)
    }
    #[classattr]
    fn L16Mono() -> Self {
        Self::from(RtpType::L16Mono)
    }
    #[classattr]
    fn Qcelp() -> Self {
        Self::from(RtpType::Qcelp)
    }
    #[classattr]
    fn Cn() -> Self {
        Self::from(RtpType::Cn)
    }
    #[classattr]
    fn Mpa() -> Self {
        Self::from(RtpType::Mpa)
    }
    #[classattr]
    fn G728() -> Self {
        Self::from(RtpType::G728)
    }
    #[classattr]
    fn G729() -> Self {
        Self::from(RtpType::G729)
    }
    #[classattr]
    fn CelB() -> Self {
        Self::from(RtpType::CelB)
    }
    #[classattr]
    fn Jpeg() -> Self {
        Self::from(RtpType::Jpeg)
    }
    #[classattr]
    fn Nv() -> Self {
        Self::from(RtpType::Nv)
    }
    #[classattr]
    fn H261() -> Self {
        Self::from(RtpType::H261)
    }
    #[classattr]
    fn Mpv() -> Self {
        Self::from(RtpType::Mpv)
    }
    #[classattr]
    fn Mp2t() -> Self {
        Self::from(RtpType::Mp2t)
    }
    #[classattr]
    fn H263() -> Self {
        Self::from(RtpType::H263)
    }
    #[classattr]
    fn Dynamic_1() -> Self {
        Self::from(RtpType::Dynamic(1))
    }
    #[classattr]
    fn Dynamic_2() -> Self {
        Self::from(RtpType::Dynamic(2))
    }
    #[classattr]
    fn Dynamic_3() -> Self {
        Self::from(RtpType::Dynamic(3))
    }
    #[classattr]
    fn Dynamic_4() -> Self {
        Self::from(RtpType::Dynamic(4))
    }
    #[classattr]
    fn Dynamic_5() -> Self {
        Self::from(RtpType::Dynamic(5))
    }
    #[classattr]
    fn Dynamic_6() -> Self {
        Self::from(RtpType::Dynamic(6))
    }
    #[classattr]
    fn Dynamic_7() -> Self {
        Self::from(RtpType::Dynamic(7))
    }
    #[classattr]
    fn Dynamic_8() -> Self {
        Self::from(RtpType::Dynamic(8))
    }
    #[classattr]
    fn Reserved_1() -> Self {
        Self::from(RtpType::Reserved(1))
    }
    #[classattr]
    fn Reserved_2() -> Self {
        Self::from(RtpType::Reserved(2))
    }
    #[classattr]
    fn Reserved_3() -> Self {
        Self::from(RtpType::Reserved(3))
    }
    #[classattr]
    fn Reserved_4() -> Self {
        Self::from(RtpType::Reserved(4))
    }
    #[classattr]
    fn Reserved_5() -> Self {
        Self::from(RtpType::Reserved(5))
    }
    #[classattr]
    fn Reserved_6() -> Self {
        Self::from(RtpType::Reserved(6))
    }
    #[classattr]
    fn Reserved_7() -> Self {
        Self::from(RtpType::Reserved(7))
    }
    #[classattr]
    fn Reserved_8() -> Self {
        Self::from(RtpType::Reserved(8))
    }
    #[classattr]
    fn Unassigned_1() -> Self {
        Self::from(RtpType::Unassigned(1))
    }
    #[classattr]
    fn Unassigned_2() -> Self {
        Self::from(RtpType::Unassigned(2))
    }
    #[classattr]
    fn Unassigned_3() -> Self {
        Self::from(RtpType::Unassigned(3))
    }
    #[classattr]
    fn Unassigned_4() -> Self {
        Self::from(RtpType::Unassigned(4))
    }
    #[classattr]
    fn Unassigned_5() -> Self {
        Self::from(RtpType::Unassigned(5))
    }
    #[classattr]
    fn Unassigned_6() -> Self {
        Self::from(RtpType::Unassigned(6))
    }
    #[classattr]
    fn Unassigned_7() -> Self {
        Self::from(RtpType::Unassigned(7))
    }
    #[classattr]
    fn Unassigned_8() -> Self {
        Self::from(RtpType::Unassigned(8))
    }
    #[classattr]
    fn Illegal_1() -> Self {
        Self::from(RtpType::Illegal(1))
    }
    #[classattr]
    fn Illegal_2() -> Self {
        Self::from(RtpType::Illegal(2))
    }
    #[classattr]
    fn Illegal_3() -> Self {
        Self::from(RtpType::Illegal(3))
    }
    #[classattr]
    fn Illegal_4() -> Self {
        Self::from(RtpType::Illegal(4))
    }
    #[classattr]
    fn Illegal_5() -> Self {
        Self::from(RtpType::Illegal(5))
    }
    #[classattr]
    fn Illegal_6() -> Self {
        Self::from(RtpType::Illegal(6))
    }
    #[classattr]
    fn Illegal_7() -> Self {
        Self::from(RtpType::Illegal(7))
    }
    #[classattr]
    fn Illegal_8() -> Self {
        Self::from(RtpType::Illegal(8))
    }

    fn __str__(&self) -> String {
        match self.rtptype {
            RtpType::Pcmu => "<RtpType.Pcmu>".to_string(),
            RtpType::Gsm => "<RtpType.Gsm>".to_string(),
            RtpType::G723 => "<RtpType.G723>".to_string(),
            RtpType::Dvi4(ix) => format!("<RtpType.Dvi4_{}>", ix),
            RtpType::Lpc => "<RtpType.Lpc>".to_string(),
            RtpType::Pcma => "<RtpType.Pcma>".to_string(),
            RtpType::G722 => "<RtpType.G722>".to_string(),
            RtpType::L16Stereo => "<RtpType.L16Stereo>".to_string(),
            RtpType::L16Mono => "<RtpType.L16Mono>".to_string(),
            RtpType::Qcelp => "<RtpType.Qcelp>".to_string(),
            RtpType::Cn => "<RtpType.Cn>".to_string(),
            RtpType::Mpa => "<RtpType.Mpa>".to_string(),
            RtpType::G728 => "<RtpType.G728>".to_string(),
            RtpType::G729 => "<RtpType.G729>".to_string(),
            RtpType::CelB => "<RtpType.CelB>".to_string(),
            RtpType::Jpeg => "<RtpType.Jpeg>".to_string(),
            RtpType::Nv => "<RtpType.Nv>".to_string(),
            RtpType::H261 => "<RtpType.H261>".to_string(),
            RtpType::Mpv => "<RtpType.Mpv>".to_string(),
            RtpType::Mp2t => "<RtpType.Mp2t>".to_string(),
            RtpType::H263 => "<RtpType.H263>".to_string(),
            RtpType::Dynamic(ix) => format!("<RtpType.Dynamic_{}>", ix),
            RtpType::Reserved(ix) => format!("<RtpType.Reserved_{}>", ix),
            RtpType::Unassigned(ix) => format!("<RtpType.Unassigned_{}>", ix),
            RtpType::Illegal(ix) => format!("<RtpType.Illegal_{}>", ix),
            _ => "<RtpType.?????>".to_string(),
        }
    }

    fn __richcmp__(&self, other: Self, op: CompareOp) -> PyResult<PyObject> {
        Python::with_gil(|py| match op {
            CompareOp::Eq => PyResult::Ok((self.rtptype == other.rtptype).into_py(py)),
            _ => PyResult::Err(PyTypeError::new_err(
                "Only __eq__ is implemented for this type",
            )),
        })
    }
}

#[pyclass(name = "Rtp")]
#[derive(Clone)]
pub struct PyRtp {
    #[pyo3(get)]
    pub version: u8,
    #[pyo3(get)]
    pub padding: u8,
    #[pyo3(get)]
    pub extension: u8,
    #[pyo3(get)]
    pub csrc_count: u8,
    #[pyo3(get)]
    pub marker: u8,
    #[pyo3(get)]
    pub payload_type: PyRtpType,
    #[pyo3(get)]
    pub sequence: u16,
    #[pyo3(get)]
    pub timestamp: u32,
    #[pyo3(get)]
    pub ssrc: u32,
    #[pyo3(get)]
    pub csrc_list: Vec<u32>,
    #[pyo3(get)]
    pub payload: Vec<u8>,
}

impl PyRtp {
    fn from(packet: &Rtp) -> Self {
        Self {
            version: packet.version,
            padding: packet.padding,
            extension: packet.extension,
            csrc_count: packet.csrc_count,
            marker: packet.marker,
            payload_type: PyRtpType::from(packet.payload_type),
            sequence: packet.sequence.into(),
            timestamp: packet.timestamp.into(),
            ssrc: packet.ssrc,
            csrc_list: packet.csrc_list.clone(),
            payload: packet.payload.clone(),
        }
    }
}

#[pyclass(name = "VoiceData")]
pub struct PyVoiceData {
    #[pyo3(get)]
    pub audio: Option<Vec<i16>>,
    #[pyo3(get)]
    pub packet: PyRtp,
    #[pyo3(get)]
    pub payload_offset: usize,
    #[pyo3(get)]
    pub payload_end_pad: usize,
}

impl PyVoiceData {
    fn from(data: &VoiceData) -> Self {
        Self {
            audio: (*data.audio).clone(),
            packet: PyRtp::from(data.packet),
            payload_offset: data.payload_offset,
            payload_end_pad: data.payload_end_pad,
        }
    }
}

#[pyclass(name = "SpeakingUpdateData")]
pub struct PySpeakingUpdateData {
    #[pyo3(get)]
    pub speaking: bool,
    #[pyo3(get)]
    pub ssrc: u32,
}

#[pyclass(name = "ClientConnect")]
pub struct PyClientConnect {
    #[pyo3(get)]
    audio_ssrc: u32,
    #[pyo3(get)]
    user_id: u64,
    #[pyo3(get)]
    video_ssrc: u32,
}

#[pyclass(name = "ConnectData")]
pub struct PyConnectData {
    #[pyo3(get)]
    channel_id: Option<u64>,
    #[pyo3(get)]
    guild_id: u64,
    #[pyo3(get)]
    session_id: String,
    #[pyo3(get)]
    server: String,
    #[pyo3(get)]
    ssrc: u32,
}

impl PyConnectData {
    fn from(connect: &ConnectData) -> Self {
        Self {
            channel_id: match connect.channel_id {
                Some(id) => Some(id.0.into()),
                None => None,
            },
            guild_id: connect.guild_id.0.into(),
            session_id: connect.session_id.to_string(),
            server: connect.server.to_string(),
            ssrc: connect.ssrc,
        }
    }
}

#[pyclass(name = "DisconnectData")]
pub struct PyDisconnectData {
    #[pyo3(get)]
    kind: PyDisconnectKind,
    #[pyo3(get)]
    reason: Option<PyDisconnectReason>,
    #[pyo3(get)]
    channel_id: Option<u64>,
    #[pyo3(get)]
    guild_id: u64,
    #[pyo3(get)]
    session_id: String,
}

#[pyclass(name = "DisconnectKind")]
#[derive(Clone)]
pub struct PyDisconnectKind {
    pub kind: DisconnectKind,
}

impl PyDisconnectKind {
    fn from(kind: DisconnectKind) -> Self {
        Self { kind }
    }
}

#[pymethods]
#[allow(non_snake_case)]
impl PyDisconnectKind {
    #[classattr]
    /// The voice driver failed to connect to the server.
    fn Connect() -> Self {
        Self::from(DisconnectKind::Connect)
    }
    #[classattr]
    /// The voice driver failed to reconnect to the server.
    fn Reconnect() -> Self {
        Self::from(DisconnectKind::Reconnect)
    }
    #[classattr]
    /// The voice connection was terminated mid-session by either the user or Discord.
    fn Runtime() -> Self {
        Self::from(DisconnectKind::Runtime)
    }
}

#[pyclass(name = "DisconnectReason")]
#[derive(Clone)]
pub struct PyDisconnectReason {
    pub reason: DisconnectReason,
}

impl PyDisconnectReason {
    fn from(reason: DisconnectReason) -> Self {
        Self { reason }
    }
}

#[allow(non_snake_case)]
#[pymethods]
impl PyDisconnectReason {
    #[classattr]
    /// This (re)connection attempt was dropped due to another request.
    fn AttemptDiscarded() -> Self {
        Self::from(DisconnectReason::AttemptDiscarded)
    }
    #[classattr]
    /// Songbird had an internal error.
    fn Internal() -> Self {
        Self::from(DisconnectReason::Internal)
    }
    #[classattr]
    /// A host-specific I/O error caused the fault; this is likely transient, and should be retried some time later.
    fn Io() -> Self {
        Self::from(DisconnectReason::Io)
    }
    #[classattr]
    /// Songbird and Discord disagreed on the protocol used to establish a voice connection.
    fn ProtocolViolation() -> Self {
        Self::from(DisconnectReason::ProtocolViolation)
    }
    #[classattr]
    /// A voice connection was not established in the specified time.
    fn TimedOut() -> Self {
        Self::from(DisconnectReason::TimedOut)
    }
    #[classattr]
    /// The Websocket connection was closed by Discord.
    fn WsClosed() -> Self {
        Self::from(DisconnectReason::WsClosed(None))
    }
}
