#[cfg(feature = "std-mode")]
mod tokio_impl {
    use std::sync::OnceLock;
    use file_manager::{IntAlloc};
    use file_manager::runtime::{Mutex, MutexGuard};
    use allocator_api2::boxed::Box;

    pub type Buf = Box<[u8; BUF_SIZE], IntAlloc>;
    pub const BUF_SIZE: usize = 2048;

    pub static BUF: OnceLock<Mutex<Buf>> = OnceLock::new();

    pub fn init_buf() {
        BUF.set(Mutex::new(Box::new_in([0u8; BUF_SIZE], IntAlloc::default()))).expect("initing twice BUF");
    }

    pub async fn get_buf() -> MutexGuard<'static, Buf> {
        BUF.get().expect("BUF not initialized").lock().await
    }
}

#[cfg(feature = "embassy-mode")]
mod embassy_impl {
    use embassy_sync::once_lock::OnceLock;
    use file_manager::{IntAlloc};
    use file_manager::runtime::{Mutex, MutexGuard};
    use allocator_api2::boxed::Box;

    pub type Buf = Box<[u8; BUF_SIZE], IntAlloc>;
    pub const BUF_SIZE: usize = 2048;

    pub static BUF: OnceLock<Mutex<Buf>> = OnceLock::new();

    pub fn init_buf() {
        let _ = BUF.init(Mutex::new(Box::new_in([0u8; BUF_SIZE], IntAlloc::default())));
    }

    pub async fn get_buf() -> MutexGuard<'static, Buf> {
        BUF.get().await.lock().await
    }
}

#[cfg(feature = "std-mode")]
pub use tokio_impl::*;

#[cfg(feature = "embassy-mode")]
pub use embassy_impl::*;

pub fn init() {
    init_buf();
}
