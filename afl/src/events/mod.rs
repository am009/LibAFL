#[cfg(feature = "std")]
pub mod llmp;
#[cfg(feature = "std")]
pub mod shmem_translated;

use alloc::string::String;
use core::{marker::PhantomData, time};

use serde::{Deserialize, Serialize};

//#[cfg(feature = "std")]
//pub mod shmem_translated;

#[cfg(feature = "std")]
use std::{io::Write, time::Duration};

use crate::corpus::Corpus;
use crate::executors::Executor;
use crate::feedbacks::FeedbacksTuple;
use crate::inputs::Input;
use crate::observers::ObserversTuple;
use crate::serde_anymap::Ptr;
use crate::utils::Rand;
use crate::AflError;
use crate::{engines::State, utils};

use self::llmp::Tag;

#[derive(Debug, Copy, Clone)]
/// Indicate if an event worked or not
pub enum BrokerEventResult {
    /// The broker haneled this. No need to pass it on.
    Handled,
    /// Pass this message along to the clients.
    Forward,
}

#[derive(Debug, Clone, Default)]
pub struct ClientStats {
    // stats (maybe we need a separated struct?)
    corpus_size: u64,
    executions: u64,
}

/*
/// A custom event, for own messages, with own handler.
pub trait CustomEvent<I>: SerdeAny
where
    I: Input,
{
    /// Returns the name of this event
    fn name(&self) -> &str;
    /// This method will be called in the broker
    fn handle_in_broker(&self) -> Result<BrokerEventResult, AflError>;
    /// This method will be called in the clients after handle_in_broker (unless BrokerEventResult::Handled) was returned in handle_in_broker
    fn handle_in_client(&self) -> Result<(), AflError>;
}
*/

/// Events sent around in the library
pub trait Event<I>
where
    I: Input,
{
    /// Returns the name of this event
    fn name(&self) -> &str;
    /// This method will be called in the broker
    fn handle_in_broker(&self, stats: &mut Stats) -> Result<BrokerEventResult, AflError>;
    /// This method will be called in the clients after handle_in_broker (unless BrokerEventResult::Handled) was returned in handle_in_broker
    fn handle_in_client<C, OT, FT, R>(
        self,
        state: &mut State<I, R, FT, OT>,
        corpus: &mut C,
    ) -> Result<(), AflError>
    where
        C: Corpus<I, R>,
        OT: ObserversTuple,
        FT: FeedbacksTuple<I>,
        R: Rand;
}

#[derive(Debug, Clone, Default)]
pub struct Stats {
    start_time: Duration,
    corpus_size: usize,
    client_stats: Vec<ClientStats>,
}

impl Stats {
    /// the client stats, mutable
    fn client_stats_mut(&mut self) -> &mut Vec<ClientStats> {
        &mut self.client_stats
    }

    /// the client stats
    fn client_stats(&self) -> &[ClientStats] {
        &self.client_stats
    }

    /// Amount of elements in the corpus (combined for all children)
    fn corpus_size(&self) -> u64 {
        self.client_stats()
            .iter()
            .fold(0u64, |acc, x| acc + x.corpus_size)
    }

    /// Time this fuzzing run stated
    fn start_time(&mut self) -> time::Duration {
        self.start_time
    }

    /// Total executions
    #[inline]
    fn total_execs(&mut self) -> u64 {
        self.client_stats()
            .iter()
            .fold(0u64, |acc, x| acc + x.executions)
    }

    /// Executions per second
    #[inline]
    fn execs_per_sec(&mut self) -> u64 {
        let time_since_start = (utils::current_time() - self.start_time()).as_secs();
        if time_since_start == 0 {
            0
        } else {
            self.total_execs() / time_since_start
        }
    }

    /// The client stats for a specific id, creating new if it doesn't exist
    fn client_stats_mut_for(&mut self, client_id: u64) -> &mut ClientStats {
        let client_stat_count = self.client_stats().len();
        for _ in client_stat_count..(client_id + 1) as usize {
            self.client_stats_mut().push(ClientStats {
                ..Default::default()
            })
        }
        &mut self.client_stats_mut()[client_id as usize]
    }
}

pub trait EventManager<C, E, OT, FT, I, R>
where
    C: Corpus<I, R>,
    E: Executor<I>,
    OT: ObserversTuple,
    FT: FeedbacksTuple<I>,
    I: Input,
    R: Rand,
{
    /// Fire an Event
    //fn fire<'a>(&mut self, event: Event<I>) -> Result<(), AflError>;

    /// Lookup for incoming events and process them.
    /// Return the number of processes events or an error
    fn process(
        &mut self,
        state: &mut State<I, R, FT, OT>,
        corpus: &mut C,
    ) -> Result<usize, AflError>;

    fn serialize_observers(&mut self, observers: &OT) -> Result<Vec<u8>, AflError> {
        Ok(postcard::to_allocvec(observers)?)
    }

    fn deserialize_observers(&mut self, observers_buf: &[u8]) -> Result<OT, AflError> {
        Ok(postcard::from_bytes(observers_buf)?)
    }

    fn new_testcase(
        &mut self,
        _input: &I,
        _observers: &OT,
        _corpus_size: usize,
        _config: String,
    ) -> Result<(), AflError> {
        Ok(())
    }

    fn update_stats(&mut self, _executions: usize, _execs_over_sec: u64) -> Result<(), AflError> {
        Ok(())
    }

    fn crash(&mut self, _input: &I) -> Result<(), AflError> {
        Ok(())
    }

    fn timeout(&mut self, _input: &I) -> Result<(), AflError> {
        Ok(())
    }

    fn log(&mut self, _severity_level: u8, _message: String) -> Result<(), AflError> {
        Ok(())
    }

    // TODO Custom event fire (dyn CustomEvent or similar)
}

#[derive(Debug)]
pub enum LoggerEvent<I>
where
    I: Input,
{
    NewTestcase {
        corpus_size: usize,
        phantom: PhantomData<I>,
    },
    UpdateStats {
        executions: usize,
        execs_over_sec: u64,
        phantom: PhantomData<I>,
    },
    Crash {
        input: I,
    },
    Timeout {
        input: I,
    },
    Log {
        severity_level: u8,
        message: String,
        phantom: PhantomData<I>,
    },
    /*Custom {
        // TODO: Allow custom events
        // custom_event: Box<dyn CustomEvent<I, OT>>,
    },*/
}

impl<I> Event<I> for LoggerEvent<I>
where
    I: Input,
{
    #[inline]
    fn name(&self) -> &str {
        match self {
            LoggerEvent::NewTestcase {
                corpus_size: _,
                phantom: _,
            } => "New Testcase",
            LoggerEvent::UpdateStats {
                executions: _,
                execs_over_sec: _,
                phantom: _,
            } => "Stats",
            LoggerEvent::Crash { input: _ } => "Crash",
            LoggerEvent::Timeout { input: _ } => "Timeout",
            LoggerEvent::Log {
                severity_level: _,
                message: _,
                phantom: _,
            } => "Log",
            /*Event::Custom => custom_event.name()
            } => "todo",*/
        }
    }

    /// Broker fun
    #[inline]
    fn handle_in_broker(&self, stats: &mut Stats) -> Result<BrokerEventResult, AflError> {
        match self {
            LoggerEvent::NewTestcase {
                corpus_size,
                phantom: _,
            } => {
                stats.client_stats_mut()[0].corpus_size = *corpus_size as u64;
                println!("[NEW] corpus: {}", stats.corpus_size());
                Ok(BrokerEventResult::Handled)
            }
            LoggerEvent::UpdateStats {
                executions,
                execs_over_sec: _,
                phantom: _,
            } => {
                // TODO: The stats buffer should be added on client add.
                stats.client_stats_mut()[0].executions = *executions as u64;
                println!(
                    "[UPDATE] corpus: {} execs: {} execs/s: {}",
                    stats.corpus_size(),
                    stats.total_execs(),
                    stats.execs_per_sec()
                );
                Ok(BrokerEventResult::Handled)
            }
            LoggerEvent::Crash { input: _ } => {
                panic!("LoggerEventManager cannot handle Event::Crash");
            }
            LoggerEvent::Timeout { input: _ } => {
                panic!("LoggerEventManager cannot handle Event::Timeout");
            }
            LoggerEvent::Log {
                severity_level,
                message,
                phantom: _,
            } => {
                println!("[LOG {}]: {}", severity_level, message);
                Ok(BrokerEventResult::Handled)
            } //_ => Ok(BrokerEventResult::Forward),
        }
    }

    #[inline]
    fn handle_in_client<C, OT, FT, R>(
        self,
        _state: &mut State<I, R, FT, OT>,
        _corpus: &mut C,
    ) -> Result<(), AflError>
    where
        C: Corpus<I, R>,
        OT: ObserversTuple,
        FT: FeedbacksTuple<I>,
        R: Rand,
    {
        match self {
            _ => Err(AflError::Unknown(format!(
                "Received illegal message that message should not have arrived: {:?}.",
                self
            ))),
        }
    }
}

pub struct LoggerEventManager<C, E, OT, FT, I, R, W>
where
    C: Corpus<I, R>,
    E: Executor<I>,
    OT: ObserversTuple,
    FT: FeedbacksTuple<I>,
    I: Input,
    R: Rand,
    W: Write,
    //CE: CustomEvent<I, OT>,
{
    writer: W,

    stats: Stats,
    events: Vec<LoggerEvent<I>>,
    // stats (maybe we need a separated struct?)
    phantom: PhantomData<(C, E, I, R, OT, FT)>,
}

impl<C, E, OT, FT, I, R, W> EventManager<C, E, OT, FT, I, R>
    for LoggerEventManager<C, E, OT, FT, I, R, W>
where
    C: Corpus<I, R>,
    E: Executor<I>,
    OT: ObserversTuple,
    FT: FeedbacksTuple<I>,
    I: Input,
    R: Rand,
    W: Write,
    //CE: CustomEvent<I, OT>,
{
    fn process(
        &mut self,
        state: &mut State<I, R, FT, OT>,
        corpus: &mut C,
    ) -> Result<usize, AflError> {
        let count = self.events.len();
        self.events
            .drain(..)
            .try_for_each(|event| event.handle_in_client(state, corpus))?;
        Ok(count)
    }

    fn new_testcase(
        &mut self,
        _input: &I,
        _observers: &OT,
        corpus_size: usize,
        _config: String,
    ) -> Result<(), AflError> {
        let event = LoggerEvent::NewTestcase {
            corpus_size: corpus_size,
            phantom: PhantomData,
        };
        match event.handle_in_broker(&mut self.stats)? {
            BrokerEventResult::Forward => self.events.push(event),
            _ => (),
        };
        Ok(())
    }

    fn update_stats(&mut self, executions: usize, execs_over_sec: u64) -> Result<(), AflError> {
        let event = LoggerEvent::UpdateStats {
            executions: executions,
            execs_over_sec: execs_over_sec,
            phantom: PhantomData,
        };
        match event.handle_in_broker(&mut self.stats)? {
            BrokerEventResult::Forward => self.events.push(event),
            _ => (),
        };
        Ok(())
    }

    fn crash(&mut self, input: &I) -> Result<(), AflError> {
        let event = LoggerEvent::Crash {
            input: input.clone(),
        };
        match event.handle_in_broker(&mut self.stats)? {
            BrokerEventResult::Forward => self.events.push(event),
            _ => (),
        };
        Ok(())
    }

    fn timeout(&mut self, input: &I) -> Result<(), AflError> {
        let event = LoggerEvent::Timeout {
            input: input.clone(),
        };
        match event.handle_in_broker(&mut self.stats)? {
            BrokerEventResult::Forward => self.events.push(event),
            _ => (),
        };
        Ok(())
    }

    fn log(&mut self, severity_level: u8, message: String) -> Result<(), AflError> {
        let event = LoggerEvent::Log {
            severity_level: severity_level,
            message: message,
            phantom: PhantomData,
        };
        match event.handle_in_broker(&mut self.stats)? {
            BrokerEventResult::Forward => self.events.push(event),
            _ => (),
        };
        Ok(())
    }
}

impl<C, E, OT, FT, I, R, W> LoggerEventManager<C, E, OT, FT, I, R, W>
where
    C: Corpus<I, R>,
    I: Input,
    E: Executor<I>,
    OT: ObserversTuple,
    FT: FeedbacksTuple<I>,
    R: Rand,
    W: Write,
    //TODO CE: CustomEvent,
{
    pub fn new(writer: W) -> Self {
        Self {
            stats: Stats {
                start_time: utils::current_time(),
                ..Default::default()
            },
            writer: writer,
            phantom: PhantomData,
            events: vec![],
        }
    }
}

#[cfg(feature = "std")]
#[derive(Serialize, Deserialize)]
#[serde(bound = "I: serde::de::DeserializeOwned")]
pub enum LLMPEventKind<'a, I>
where
    I: Input,
{
    NewTestcase {
        input: Ptr<'a, I>,
        observers_buf: Vec<u8>,
        corpus_size: usize,
        client_config: String,
    },
    UpdateStats {
        executions: usize,
        execs_over_sec: u64,
        phantom: PhantomData<&'a I>,
    },
    Crash {
        input: I,
    },
    Timeout {
        input: I,
    },
    Log {
        severity_level: u8,
        message: String,
        phantom: PhantomData<I>,
    },
    /*Custom {
        // TODO: Allow custom events
        // custom_event: Box<dyn CustomEvent<I, OT>>,
    },*/
}

#[cfg(feature = "std")]
#[derive(Serialize, Deserialize)]
#[serde(bound = "I: serde::de::DeserializeOwned")]
pub struct LLMPEvent<'a, I>
where
    I: Input,
{
    sender_id: u64,
    kind: LLMPEventKind<'a, I>,
}

#[cfg(feature = "std")]
impl<'a, I> Event<I> for LLMPEvent<'a, I>
where
    I: Input,
{
    fn name(&self) -> &str {
        match self.kind {
            LLMPEventKind::NewTestcase {
                input: _,
                client_config: _,
                corpus_size: _,
                observers_buf: _,
            } => "New Testcase",
            LLMPEventKind::UpdateStats {
                executions: _,
                execs_over_sec: _,
                phantom: _,
            } => "Stats",
            LLMPEventKind::Crash { input: _ } => "Crash",
            LLMPEventKind::Timeout { input: _ } => "Timeout",
            LLMPEventKind::Log {
                severity_level: _,
                message: _,
                phantom: _,
            } => "Log",
            /*Event::Custom {
                sender_id: _, /*custom_event} => custom_event.name()*/
            } => "todo",*/
        }
    }

    /// Broker fun
    #[inline]
    fn handle_in_broker(&self, stats: &mut Stats) -> Result<BrokerEventResult, AflError> {
        match &self.kind {
            LLMPEventKind::NewTestcase {
                input: _,
                client_config: _,
                corpus_size,
                observers_buf: _,
            } => {
                let client = stats.client_stats_mut_for(self.sender_id);
                client.corpus_size = *corpus_size as u64;
                println!("[NEW] corpus: {}", stats.corpus_size());
                Ok(BrokerEventResult::Handled)
            }
            LLMPEventKind::UpdateStats {
                executions,
                execs_over_sec: _,
                phantom: _,
            } => {
                // TODO: The stats buffer should be added on client add.
                let client = stats.client_stats_mut_for(self.sender_id);
                client.executions = *executions as u64;
                println!(
                    "[UPDATE] corpus: {} execs: {} execs/s: {}",
                    stats.corpus_size(),
                    stats.total_execs(),
                    stats.execs_per_sec()
                );
                Ok(BrokerEventResult::Handled)
            }
            LLMPEventKind::Crash { input: _ } => {
                panic!("LoggerEventManager cannot handle Event::Crash");
            }
            LLMPEventKind::Timeout { input: _ } => {
                panic!("LoggerEventManager cannot handle Event::Timeout");
            }
            LLMPEventKind::Log {
                severity_level,
                message,
                phantom: _,
            } => {
                println!("[LOG {}]: {}", severity_level, message);
                Ok(BrokerEventResult::Handled)
            }
            _ => Ok(BrokerEventResult::Forward),
        }
    }

    #[inline]
    fn handle_in_client<C, OT, FT, R>(
        self,
        state: &mut State<I, R, FT, OT>,
        corpus: &mut C,
    ) -> Result<(), AflError>
    where
        C: Corpus<I, R>,
        OT: ObserversTuple,
        FT: FeedbacksTuple<I>,
        R: Rand,
    {
        match self.kind {
            LLMPEventKind::NewTestcase {
                input,
                client_config: _,
                corpus_size: _,
                observers_buf,
            } => {
                // TODO: here u should match client_config, if equal to the current one do not re-execute
                // we need to pass engine to process() too, TODO
                #[cfg(feature = "std")]
                println!("Received new Testcase");
                let observers = postcard::from_bytes(&observers_buf)?;
                let interestingness = state.is_interesting(input.as_ref(), &observers)?;
                match input {
                    Ptr::Owned(b) => {
                        state.add_if_interesting(corpus, *b, interestingness)?;
                    }
                    _ => {}
                };
                Ok(())
            }
            _ => Err(AflError::Unknown(format!(
                "Received illegal message that message should not have arrived: {:?}.",
                self.name()
            ))),
        }
    }
}

#[cfg(feature = "std")]
/// Forward this to the client
const _LLMP_TAG_EVENT_TO_CLIENT: llmp::Tag = 0x2C11E471;
#[cfg(feature = "std")]
/// Only handle this in the broker
const _LLMP_TAG_EVENT_TO_BROKER: llmp::Tag = 0x2B80438;
#[cfg(feature = "std")]
/// Handle in both
const LLMP_TAG_EVENT_TO_BOTH: llmp::Tag = 0x2B0741;

#[cfg(feature = "std")]
pub struct LlmpEventManager<C, E, OT, FT, I, R, W>
where
    W: Write,
    //CE: CustomEvent<I>,
{
    writer: W,

    // stats (maybe we need a separated struct?)
    llmp: llmp::LlmpConnection,
    stats: Stats,
    phantom: PhantomData<(C, E, OT, FT, I, R)>,
}

impl<C, E, OT, FT, I, R, W> LlmpEventManager<C, E, OT, FT, I, R, W>
where
    C: Corpus<I, R>,
    E: Executor<I>,
    OT: ObserversTuple,
    FT: FeedbacksTuple<I>,
    I: Input,
    R: Rand,
    W: Write,
{
    /// Create llmp on a port
    /// If the port is not yet bound, it will act as broker
    /// Else, it will act as client.
    pub fn new_on_port(port: u16, writer: W) -> Result<Self, AflError> {
        let mgr = Self {
            llmp: llmp::LlmpConnection::on_port(port)?,
            stats: Stats {
                start_time: utils::current_time(),
                ..Default::default()
            },
            phantom: PhantomData,
            writer,
        };
        Ok(mgr)
    }

    /// Returns if we are the broker
    pub fn is_broker(&self) -> bool {
        match self.llmp {
            llmp::LlmpConnection::IsBroker {
                broker: _,
                listener_thread: _,
            } => true,
            _ => false,
        }
    }

    /// Run forever in the broker
    pub fn broker_loop(&mut self) -> Result<(), AflError> {
        match &mut self.llmp {
            llmp::LlmpConnection::IsBroker {
                broker,
                listener_thread: _,
            } => {
                let stats = &mut self.stats;
                broker.loop_forever(
                    &mut |_client_id: u32, tag: Tag, msg: &[u8]| {
                        if tag == LLMP_TAG_EVENT_TO_BOTH {
                            let event: LLMPEvent<I> = postcard::from_bytes(msg)?;
                            match event.handle_in_broker(stats)? {
                                BrokerEventResult::Forward => {
                                    Ok(llmp::LlmpMsgHookResult::ForwardToClients)
                                }
                                BrokerEventResult::Handled => Ok(llmp::LlmpMsgHookResult::Handled),
                            }
                        } else {
                            Ok(llmp::LlmpMsgHookResult::ForwardToClients)
                        }
                    },
                    Some(Duration::from_millis(5)),
                );
            }
            _ => Err(AflError::IllegalState(
                "Called broker loop in the client".into(),
            )),
        }
    }

    #[inline]
    fn llmp_send<'a>(&mut self, event: LLMPEvent<'a, I>) -> Result<(), AflError> {
        let serialized = postcard::to_allocvec(&event)?;
        self.llmp.send_buf(LLMP_TAG_EVENT_TO_BOTH, &serialized)?;
        Ok(())
    }
}

#[cfg(feature = "std")]
impl<C, E, OT, FT, I, R, W> EventManager<C, E, OT, FT, I, R>
    for LlmpEventManager<C, E, OT, FT, I, R, W>
where
    C: Corpus<I, R>,
    E: Executor<I>,
    FT: FeedbacksTuple<I>,
    OT: ObserversTuple,
    I: Input,
    R: Rand,
    W: Write,
    //CE: CustomEvent<I>,
{
    fn process(
        &mut self,
        state: &mut State<I, R, FT, OT>,
        corpus: &mut C,
    ) -> Result<usize, AflError> {
        // TODO: Get around local event copy by moving handle_in_client
        Ok(match &mut self.llmp {
            llmp::LlmpConnection::IsClient { client } => {
                let mut count = 0;
                loop {
                    match client.recv_buf()? {
                        Some((tag, event_buf)) => {
                            if tag == _LLMP_TAG_EVENT_TO_BROKER {
                                continue;
                            }
                            let event: LLMPEvent<I> = postcard::from_bytes(event_buf)?;
                            event.handle_in_client(state, corpus)?;
                            count += 1;
                        }
                        None => break count,
                    }
                }
            }
            _ => {
                dbg!("Skipping process in broker");
                0
            }
        })
    }

    fn new_testcase(
        &mut self,
        input: &I,
        observers: &OT,
        corpus_size: usize,
        config: String,
    ) -> Result<(), AflError> {
        let event = LLMPEvent {
            sender_id: 0,
            kind: LLMPEventKind::NewTestcase {
                input: Ptr::Ref(input),
                observers_buf: postcard::to_allocvec(observers)?,
                corpus_size: corpus_size,
                client_config: config,
            },
        };
        self.llmp_send(event)
    }

    fn update_stats(&mut self, executions: usize, execs_over_sec: u64) -> Result<(), AflError> {
        let event = LLMPEvent {
            sender_id: 0,
            kind: LLMPEventKind::UpdateStats {
                executions: executions,
                execs_over_sec: execs_over_sec,
                phantom: PhantomData,
            },
        };
        self.llmp_send(event)
    }

    fn crash(&mut self, input: &I) -> Result<(), AflError> {
        let event = LLMPEvent {
            sender_id: 0,
            kind: LLMPEventKind::Crash {
                input: input.clone(),
            },
        };
        self.llmp_send(event)
    }

    fn timeout(&mut self, input: &I) -> Result<(), AflError> {
        let event = LLMPEvent {
            sender_id: 0,
            kind: LLMPEventKind::Timeout {
                input: input.clone(),
            },
        };
        self.llmp_send(event)
    }

    fn log(&mut self, severity_level: u8, message: String) -> Result<(), AflError> {
        let event = LLMPEvent {
            sender_id: 0,
            kind: LLMPEventKind::Log {
                severity_level: severity_level,
                message: message,
                phantom: PhantomData,
            },
        };
        self.llmp_send(event)
    }
}

#[cfg(feature = "std")]
#[cfg(test)]
mod tests {

    use crate::events::{LLMPEvent, LLMPEventKind};
    use crate::inputs::bytes::BytesInput;
    use crate::observers::ObserversTuple;
    use crate::observers::StdMapObserver;
    use crate::serde_anymap::Ptr;
    use crate::tuples::{tuple_list, MatchNameAndType, Named};

    static mut MAP: [u32; 4] = [0; 4];
    #[test]
    fn test_event_serde() {
        let obv = StdMapObserver::new("test", unsafe { &mut MAP });
        let map = tuple_list!(obv);
        let observers_buf = map.serialize().unwrap();
        // test_event_mgr.serialize_observers(&map).unwrap();

        let i = BytesInput::new(vec![0]);
        let e = LLMPEvent {
            sender_id: 0,
            kind: LLMPEventKind::NewTestcase {
                input: Ptr::Ref(&i),
                observers_buf,
                corpus_size: 123,
                client_config: "conf".into(),
            },
        };

        let serialized = postcard::to_allocvec(&e).unwrap();

        let d = postcard::from_bytes::<LLMPEvent<BytesInput>>(&serialized).unwrap();
        match d.kind {
            LLMPEventKind::NewTestcase {
                input: _,
                observers_buf,
                corpus_size: _,
                client_config: _,
            } => {
                let o = map.deserialize(&observers_buf).unwrap();
                let test_observer = o.match_name_type::<StdMapObserver<u32>>("test").unwrap();
                assert_eq!("test", test_observer.name());
            }
            _ => panic!("mistmatch".to_string()),
        };
    }
}
