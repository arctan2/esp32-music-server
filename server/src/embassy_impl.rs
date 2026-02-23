pub use embassy_sync::once_lock::OnceLock;
pub use esp_println::println;
pub use embassy_futures::select::{select, Select, Either};
use file_manager::runtime::{Channel, Signal};
use file_manager::{ExtAlloc};
use crate::chunks::*;
use allocator_api2::boxed::Box;

pub type ChunkChan = Channel<Box<Chunk, ExtAlloc>, CHAN_CAP>;

pub static FREE_CHAN: OnceLock<ChunkChan> = OnceLock::new();
pub static READY_CHAN: OnceLock<ChunkChan> = OnceLock::new();

pub fn init_chunks(free_chan: ChunkChan, ready: ChunkChan) {
    let _ = FREE_CHAN.init(free_chan);
    let _ = READY_CHAN.init(ready);
}

pub async fn get_free_chan() -> &'static ChunkChan {
    FREE_CHAN.get().await
}

pub async fn get_ready_chan() -> &'static ChunkChan {
    READY_CHAN.get().await
}

static EVENT_SIG: OnceLock<Signal<UploadEvent>> = OnceLock::new();
static RET_SIG: OnceLock<Signal<Result<(), &'static str>>> = OnceLock::new();

pub fn init_signals() {
    let _ = EVENT_SIG.init(Signal::new());
    let _ = RET_SIG.init(Signal::new());
}

pub async fn get_event_sig() -> &'static Signal<UploadEvent> {
    EVENT_SIG.get().await
}

pub async fn get_ret_sig() -> &'static Signal<Result<(), &'static str>> {
    RET_SIG.get().await
}
