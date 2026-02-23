#![allow(unused)]
pub use std::sync::OnceLock;
pub use std::println;
pub use tokio::select;
use file_manager::{ExtAlloc, AsyncRootFn, FManError};
use allocator_api2::boxed::Box;
use file_manager::runtime::{Channel, Signal, Mutex};
use file_manager::{BlkDev, DummyTimesource};

use crate::chunks::*;

pub type ChunkChan = Channel<Box<Chunk, ExtAlloc>, CHAN_CAP>;

pub static FREE_CHAN: OnceLock<ChunkChan> = OnceLock::new();
pub static READY_CHAN: OnceLock<ChunkChan> = OnceLock::new();

pub fn init_chunks(free_chan: ChunkChan, ready: ChunkChan) {
    FREE_CHAN.set(free_chan).expect("initing free twice sender");
    READY_CHAN.set(ready).expect("initing free twice sender");
}

pub async fn get_free_chan() -> &'static ChunkChan {
    FREE_CHAN.get().expect("get_free_chan not initialized")
}

pub async fn get_ready_chan() -> &'static ChunkChan {
    READY_CHAN.get().expect("get_free_chan not initialized")
}

static EVENT_SIG: OnceLock<Signal<UploadEvent>> = OnceLock::new();
static RET_SIG: OnceLock<Signal<Result<(), &'static str>>> = OnceLock::new();

pub fn init_signals() {
    EVENT_SIG.set(Signal::new()).expect("initing free twice sender");
    RET_SIG.set(Signal::new()).expect("initing free twice sender");
}

pub async fn get_event_sig() -> &'static Signal<UploadEvent> {
    EVENT_SIG.get().unwrap()
}

pub async fn get_ret_sig() -> &'static Signal<Result<(), &'static str>> {
    RET_SIG.get().unwrap()
}
