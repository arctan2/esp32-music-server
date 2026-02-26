#[cfg(feature = "std-mode")]
mod tokio_impl {
    use std::sync::OnceLock;
    use file_manager::{IntAlloc, ExtAlloc};
    use file_manager::runtime::{Mutex, MutexGuard};
    use allocator_api2::boxed::Box;

    pub const BUF_SIZE: usize = 2048;
    pub type BufInt = Box<[u8; BUF_SIZE], IntAlloc>;
    pub type BufExt = Box<[u8; BUF_SIZE], ExtAlloc>;

    pub static BUF: OnceLock<Mutex<BufInt>> = OnceLock::new();
    pub static BUF2: OnceLock<Mutex<BufExt>> = OnceLock::new();

    pub fn init_bufs() {
        BUF.set(Mutex::new(Box::new_in([0u8; BUF_SIZE], IntAlloc::default()))).expect("initing twice BUF");
        BUF2.set(Mutex::new(Box::new_in([0u8; BUF_SIZE], ExtAlloc::default()))).expect("initing twice BUF2");
    }

    pub async fn get_buf() -> MutexGuard<'static, BufInt> {
        BUF.get().expect("BUF not initialized").lock().await
    }

    pub async fn get_buf2() -> MutexGuard<'static, BufExt> {
        BUF2.get().expect("BUF2 not initialized").lock().await
    }
}

#[cfg(feature = "embassy-mode")]
mod embassy_impl {
    use embassy_sync::once_lock::OnceLock;
    use file_manager::{IntAlloc, ExtAlloc};
    use file_manager::runtime::{Mutex, MutexGuard};
    use allocator_api2::boxed::Box;

    pub const BUF_SIZE: usize = 2048;
    pub type BufInt = Box<[u8; BUF_SIZE], IntAlloc>;
    pub type BufExt = Box<[u8; BUF_SIZE], ExtAlloc>;

    pub static BUF: OnceLock<Mutex<BufInt>> = OnceLock::new();
    pub static BUF2: OnceLock<Mutex<BufExt>> = OnceLock::new();

    pub fn init_bufs() {
        let _ = BUF.init(Mutex::new(Box::new_in([0u8; BUF_SIZE], IntAlloc::default())));
        let _ = BUF2.init(Mutex::new(Box::new_in([0u8; BUF_SIZE], ExtAlloc::default())));
    }

    pub async fn get_buf() -> MutexGuard<'static, BufInt> {
        BUF.get().await.lock().await
    }

    pub async fn get_buf2() -> MutexGuard<'static, BufExt> {
        BUF2.get().await.lock().await
    }
}

#[cfg(feature = "std-mode")]
pub use tokio_impl::*;

#[cfg(feature = "embassy-mode")]
pub use embassy_impl::*;

pub fn init() {
    init_bufs();
}
