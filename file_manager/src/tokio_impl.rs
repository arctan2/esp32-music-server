use super::*;
use allocator_api2::alloc::{Allocator, AllocError, Layout};
use core::ptr::NonNull;
use alpa::embedded_sdmmc_ram_device::esp_alloc::{ExternalMemory, InternalMemory};
pub use alpa::embedded_sdmmc_ram_device::{
    allocators,
};
use alpa::embedded_sdmmc_fs::VM;
pub use alpa::embedded_sdmmc_ram_device::block_device::{FsBlockDeviceError, FsBlockDevice};
pub use std::sync::OnceLock;

pub struct ExternalAlloc(pub allocators::SimAllocator<23>);

impl ExternalAlloc {
    pub fn default() -> Self {
        Self(ExternalMemory)
    }
}

impl Clone for ExternalAlloc {
    fn clone(&self) -> Self {
        ExternalAlloc(ExternalMemory)
    }
}

unsafe impl Allocator for ExternalAlloc {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.0.allocate(layout)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        unsafe {
            self.0.deallocate(ptr, layout)
        }
    }
}

pub struct InternalAlloc(pub allocators::SimAllocator<17>);

impl InternalAlloc {
    pub fn default() -> Self {
        Self(InternalMemory)
    }
}

impl Clone for InternalAlloc {
    fn clone(&self) -> Self {
        InternalAlloc(InternalMemory)
    }
}

unsafe impl Allocator for InternalAlloc {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.0.allocate(layout)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        unsafe {
            self.0.deallocate(ptr, layout)
        }
    }
}

pub type BlkDev = FsBlockDevice;
pub type ExtAlloc = ExternalAlloc;
pub type IntAlloc = InternalAlloc;
pub type FMan = FileManager;
pub type FsError = FsBlockDeviceError;

#[derive(Debug)]
pub struct SyncFMan(pub FMan);

unsafe impl Send for SyncFMan {}
unsafe impl Sync for SyncFMan {}

pub static FILE_MAN: OnceLock<SyncFMan> = OnceLock::new();

pub fn init_file_manager(block_device: BlkDev, time_src: DummyTimesource) {
    FILE_MAN.set(
        SyncFMan(FileManager::new(block_device, time_src))
    ).expect("initing twice file_manager");
}

pub fn get_file_manager() -> &'static FMan {
    &FILE_MAN.get().expect("file_manager not initialized").0
}

#[allow(dead_code)]
#[derive(Debug)]
pub enum InitError {
    SdCard(embedded_sdmmc::Error<FsError>),
    FManErr(FManError<FsBlockDeviceError>),
    DbErr(alpa::db::Error<embedded_sdmmc::Error<FsError>>),
}

impl From<alpa::db::Error<embedded_sdmmc::Error<FsError>>> for InitError {
    fn from(e: alpa::db::Error<embedded_sdmmc::Error<FsError>>) -> Self {
        InitError::DbErr(e)
    }
}

impl From<embedded_sdmmc::Error<FsError>> for InitError {
    fn from(e: embedded_sdmmc::Error<FsError>) -> Self {
        InitError::SdCard(e)
    }
}

impl From<FManError<FsBlockDeviceError>> for InitError {
    fn from(e: FManError<FsBlockDeviceError>) -> Self {
        InitError::FManErr(e)
    }
}

pub async fn init_file_system() -> Result<(), InitError>
where 
    embedded_sdmmc::Error<<FsBlockDevice as BlockDevice>::Error>: Into<embedded_sdmmc::Error<FsError>>
{
    let fman = get_file_manager();
    fman.with_vol_man(|vm, vol| -> Result<(), FManError<FsBlockDeviceError>> {
        let root_dir = FileManager::root_dir(vm, vol)?.to_directory(vm);
        let _ = root_dir.make_dir_in_dir(consts::DB_DIR);
        let _ = root_dir.make_dir_in_dir(consts::FILES_DIR);
        let _ = root_dir.make_dir_in_dir(consts::MUSIC_DIR);

        {
            let db_dir = root_dir.open_dir(consts::DB_DIR)?;
            let db_dir = db_dir.to_raw_directory();
            let stuff_dir = DbDirSdmmc::new(db_dir);
            let mut db = Database::new_init(VM::new(vm), stuff_dir, ExtAlloc::default())?;

            {
                let name = Column::new("name", ColumnType::Chars).primary();
                let count = Column::new("count", ColumnType::Int);
                db.new_table_begin(consts::COUNT_TRACKER_TABLE);
                db.add_column(name)?;
                db.add_column(count)?;
                let _ = db.create_table(ExtAlloc::default())?;
            }

            {
                let name = Column::new("path", ColumnType::Chars).primary();
                let count = Column::new("name", ColumnType::Chars);
                let size = Column::new("size", ColumnType::Int);
                db.new_table_begin(consts::FILES_TABLE);
                db.add_column(name)?;
                db.add_column(count)?;
                db.add_column(size)?;
                let _ = db.create_table(ExtAlloc::default())?;
            }

            {
                let name = Column::new("path", ColumnType::Chars).primary();
                let count = Column::new("name", ColumnType::Chars);
                let size = Column::new("size", ColumnType::Int);
                db.new_table_begin(consts::MUSIC_TABLE);
                db.add_column(name)?;
                db.add_column(count)?;
                db.add_column(size)?;
                let _ = db.create_table(ExtAlloc::default())?;
            }

            let count_tracker = db.get_table(consts::COUNT_TRACKER_TABLE, ExtAlloc::default())?;

            {
                let mut row = Row::new_in(ExtAlloc::default());
                row.push(Value::Chars(consts::FILES_TABLE.as_bytes()));
                row.push(Value::Int(1));
                db.insert_to_table(count_tracker, row, ExtAlloc::default())?;
            }

            {
                let mut row = Row::new_in(ExtAlloc::default());
                row.push(Value::Chars(consts::MUSIC_TABLE.as_bytes()));
                row.push(Value::Int(1));
                db.insert_to_table(count_tracker, row, ExtAlloc::default())?;
            }

            Ok(())
        }
    }).await?;
    Ok(())
}
