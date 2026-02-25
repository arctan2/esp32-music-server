use super::*;
use esp_hal::{
    gpio::{Output},
    spi::{master::{Spi}},
    delay::{Delay},
};
use esp_hal::Blocking;
use embedded_hal_bus::spi::ExclusiveDevice;
use allocator_api2::alloc::{Allocator, AllocError, Layout};
use core::ptr::NonNull;
use esp_println::{println};
pub use embedded_sdmmc::{SdCard, SdCardError};
pub use embassy_sync::once_lock::OnceLock;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use alpa::embedded_sdmmc_fs::VM;

static GLOBAL_ALLOC_LOCK: Mutex<CriticalSectionRawMutex, ()> = Mutex::new(());

pub struct ExternalAlloc;

impl ExternalAlloc {
    pub fn default() -> Self {
        Self
    }
}

impl Clone for ExternalAlloc {
    fn clone(&self) -> Self {
        Self
    }
}

unsafe impl Allocator for ExternalAlloc {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        GLOBAL_ALLOC_LOCK.lock(|_| {
            esp_alloc::ExternalMemory.allocate(layout)
        })
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        let _ = GLOBAL_ALLOC_LOCK.lock(|_| {
            unsafe {
                esp_alloc::ExternalMemory.deallocate(ptr, layout)
            }
        });
    }
}

pub struct InternalAlloc;

impl InternalAlloc {
    pub fn default() -> Self {
        Self
    }
}

impl Clone for InternalAlloc {
    fn clone(&self) -> Self {
        Self
    }
}

unsafe impl Allocator for InternalAlloc {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        esp_alloc::InternalMemory.allocate(layout)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        unsafe {
            esp_alloc::InternalMemory.deallocate(ptr, layout)
        }
    }
}

pub type ConcreteSpi<'a> = ExclusiveDevice<Spi<'a, Blocking>, Output<'a>, Delay>;
pub type ConcreteDelay = Delay;
pub type FsBlockDevice = SdCard<ConcreteSpi<'static>, ConcreteDelay>;

pub type BlkDev = FsBlockDevice;
pub type ExtAlloc = ExternalAlloc;
pub type IntAlloc = InternalAlloc;
pub type FMan = FileManager;
pub type FsError = embedded_sdmmc::SdCardError;

pub struct SyncFMan(pub FMan);
unsafe impl Send for SyncFMan {}
unsafe impl Sync for SyncFMan {}

pub static FILE_MAN: OnceLock<SyncFMan> = OnceLock::new();

pub fn init_file_manager(block_device: BlkDev, time_src: DummyTimesource)
{
    let _ = FILE_MAN.init(SyncFMan(FileManager::new(block_device, time_src)));
}

pub async fn get_file_manager() -> &'static FMan {
    &FILE_MAN.get().await.0
}

#[allow(dead_code)]
#[derive(Debug)]
pub enum InitError {
    SdCard(embedded_sdmmc::Error<FsError>),
    Database(alpa::db::Error<embedded_sdmmc::Error<FsError>>),
    FileMan(FManError<SdCardError>),
}

impl From<alpa::db::Error<embedded_sdmmc::Error<FsError>>> for InitError {
    fn from(e: alpa::db::Error<embedded_sdmmc::Error<FsError>>) -> Self {
        InitError::Database(e)
    }
}

impl From<embedded_sdmmc::Error<FsError>> for InitError {
    fn from(e: embedded_sdmmc::Error<FsError>) -> Self {
        InitError::SdCard(e)
    }
}

impl From<FManError<SdCardError>> for InitError {
    fn from(e: FManError<SdCardError>) -> Self {
        InitError::FileMan(e)
    }
}

pub async fn init_file_system(spi_device: ConcreteSpi<'static>, delay: ConcreteDelay) -> Result<(), InitError>
where 
    embedded_sdmmc::Error<<FsBlockDevice as BlockDevice>::Error>: Into<embedded_sdmmc::Error<FsError>>
{
    let sdcard = BlkDev::new(spi_device, delay);
    init_file_manager(sdcard, DummyTimesource);

    let fman = get_file_manager().await;

    fman.with_vol_man(|vm, vol| {
        let root_dir = FileManager::root_dir(vm, vol)?
                                  .to_directory(vm);
        let _ = root_dir.make_dir_in_dir(consts::DB_DIR).or_else(|e| {
            if matches!(e, embedded_sdmmc::Error::DirAlreadyExists) {
                Ok(())
            } else {
                Err(e)
            }
        })?;
        let _ = root_dir.make_dir_in_dir(consts::FILES_DIR).or_else(|e| {
            if matches!(e, embedded_sdmmc::Error::DirAlreadyExists) {
                Ok(())
            } else {
                Err(e)
            }
        })?;
        let _ = root_dir.make_dir_in_dir(consts::MUSIC_DIR).or_else(|e| {
            if matches!(e, embedded_sdmmc::Error::DirAlreadyExists) {
                Ok(())
            } else {
                Err(e)
            }
        })?;

        println!("created all dirs");

        {
            let db_dir = DbDirSdmmc::new(root_dir.open_dir(consts::DB_DIR)?.to_raw_directory());

            let mut db = Database::new_init(VM::new(vm), db_dir, ExtAlloc::default())?;
            println!("db init success");

            {
                let name = Column::new("name", ColumnType::Chars).primary();
                let count = Column::new("count", ColumnType::Int);
                db.new_table_begin(consts::COUNT_TRACKER_TABLE);
                db.add_column(name)?;
                db.add_column(count)?;
                let _ = db.create_table(ExtAlloc::default()).or_else(|e| {
                    if matches!(e, alpa::db::Error::DuplicateKey) {
                        Ok(0)
                    } else {
                        Err(e)
                    }
                })?;
            }

            println!("count_tracker done");

            let stats: esp_alloc::HeapStats = esp_alloc::HEAP.stats();
            println!("{}", stats);

            {
                let name = Column::new("path", ColumnType::Chars).primary();
                let count = Column::new("name", ColumnType::Chars);
                let size = Column::new("size", ColumnType::Int);
                db.new_table_begin(consts::FILES_TABLE);
                db.add_column(name)?;
                db.add_column(count)?;
                db.add_column(size)?;
                let _ = db.create_table(ExtAlloc::default()).or_else(|e| {
                    if matches!(e, alpa::db::Error::DuplicateKey) {
                        Ok(0)
                    } else {
                        Err(e)
                    }
                })?;
            }

            println!("files table done");

            {
                let name = Column::new("path", ColumnType::Chars).primary();
                let count = Column::new("name", ColumnType::Chars);
                let size = Column::new("size", ColumnType::Int);
                db.new_table_begin(consts::MUSIC_TABLE);
                db.add_column(name)?;
                db.add_column(count)?;
                db.add_column(size)?;
                let _ = db.create_table(ExtAlloc::default()).or_else(|e| {
                    if matches!(e, alpa::db::Error::DuplicateKey) {
                        Ok(0)
                    } else {
                        Err(e)
                    }
                })?;
            }
            println!("music table done");

            let count_tracker = db.get_table(consts::COUNT_TRACKER_TABLE, ExtAlloc::default())?;

            {
                let mut row = Row::new_in(ExtAlloc::default());
                row.push(Value::Chars(consts::FILES_TABLE.as_bytes()));
                row.push(Value::Int(1));
                let _ = db.insert_to_table(count_tracker, row, ExtAlloc::default()).or_else(|e| {
                    if matches!(e, alpa::db::Error::DuplicateKey) {
                        Ok(())
                    } else {
                        Err(e)
                    }
                })?;
            }

            println!("insert files_table to count_tracker table done");

            {
                let mut row = Row::new_in(ExtAlloc::default());
                row.push(Value::Chars(consts::MUSIC_TABLE.as_bytes()));
                row.push(Value::Int(1));
                let _ = db.insert_to_table(count_tracker, row, ExtAlloc::default()).or_else(|e| {
                    if matches!(e, alpa::db::Error::DuplicateKey) {
                        Ok(())
                    } else {
                        Err(e)
                    }
                })?;
            }
            println!("insert music_table to count_tracker table done");

            println!("closed db successfully");

            Ok(())
        }
    }).await?;
    Ok(())
}
