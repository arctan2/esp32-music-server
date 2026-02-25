#![allow(nonstandard_style)]
#![feature(impl_trait_in_assoc_type)]

#![no_std]
extern crate alloc;

#[cfg(feature = "std-mode")]
extern crate std;

pub(crate) mod internal_prelude {
    #![allow(unused)]
    pub use alloc::string::{String, ToString};
    pub use core::result::Result::{self, Ok, Err};
    pub use core::option::Option::{self, Some, None};
}
use internal_prelude::*;
use alloc::format;

pub mod chunk_receiver;
pub mod delete;
pub mod upload;
mod fs;

use alpa::embedded_sdmmc_fs::{DbDirSdmmc, VM};
use alpa::db::Database;
use alpa::{Query, QueryExecutor, Value};
use embedded_sdmmc::{BlockDevice, RawDirectory, VolumeManager};
use picoserve::routing::{PathDescription};
use picoserve::response::{IntoResponse};
use picoserve::request::{RequestBody, RequestParts, Path};
use picoserve::extract::{FromRequest};
use picoserve::io::Read;
use allocator_api2::vec::Vec;
use picoserve::response::chunked::{ChunksWritten, ChunkedResponse, ChunkWriter, Chunks};
use file_manager::{
    FMan,
    BlkDev,
    ExtAlloc,
    get_file_manager,
    FManError,
    FileType,
    CardState,
    consts,
    AsyncRootFn,
    DummyTimesource,
    FsBlockDevice
};

pub static HOME_PAGE: &str = include_str!("./html/home.html");
pub static MUSIC_LIST_PAGE: &str = include_str!("./html/music_list.html");

#[derive(Copy, Clone, Debug)]
pub struct CatchAll;

impl<T: Copy + core::fmt::Debug> PathDescription<T> for CatchAll {
    type NewPathParameters = String;

    fn parse_and_validate<'r, U, F: FnOnce(Self::NewPathParameters, Path<'r>) -> Result<U, Self::NewPathParameters>>(
        &self,
        current_path_parameters: T,
        path: Path<'r>,
        validate: F,
    ) -> Result<U, T> {
        let remaining = String::from(path.encoded());
        
        let mut empty = path;
        while let Some(p) = empty.split_first_segment() {
            empty = p.1;
        }
        
        validate(remaining, empty).map_err(|_| current_path_parameters)
    }
}

struct HandleMusicAsync<W: picoserve::io::Write> {
    chunk_writer: ChunkWriter<W>,
}

impl<W> AsyncRootFn<Result<ChunksWritten, W::Error>> for HandleMusicAsync<W>
where W: picoserve::io::Write,
{
    type Fut<'a> = impl core::future::Future<
        Output = Result<Result<ChunksWritten, W::Error>, FManError<<FsBlockDevice as BlockDevice>::Error>>>
        + 'a where Self: 'a;

    fn call<'a>(mut self, root_dir: RawDirectory, vm: &'a VolumeManager<BlkDev, DummyTimesource, 4, 4, 1>) -> Self::Fut<'a> {
        async move {
            let root_dir = root_dir.to_directory(vm);

            match root_dir.open_dir(consts::DB_DIR) {
                Ok(dir) => {
                    let db_dir = DbDirSdmmc::new(dir.to_raw_directory());
                    let vm = VM::new(vm);
                    let mut db = match Database::new_init(vm, db_dir, ExtAlloc::default()) {
                        Ok(d) => d,
                        Err(e) => {
                            if let Err(e) = self.chunk_writer.write_chunk(format!("error: {:?}", e).as_bytes()).await {
                                return Ok(Err(e));
                            }
                            return Ok(self.chunk_writer.finalize().await);
                        }
                    };
                

                    let files_table = match db.get_table(consts::MUSIC_TABLE, ExtAlloc::default()) {
                        Ok(t) => t,
                        Err(e) => {
                            if let Err(e) = self.chunk_writer.write_chunk(format!("table not found: {:?}", e).as_bytes()).await {
                                return Ok(Err(e));
                            }
                            return Ok(self.chunk_writer.finalize().await);
                        }
                    };

                    {
                        let query = Query::<_, &str>::new(files_table, ExtAlloc::default());
                        match QueryExecutor::new(
                            query, &mut db.table_buf, &mut db.buf1, &mut db.buf2,
                            &db.file_handler.page_rw.as_ref().unwrap()
                        ) {
                            Ok(mut exec) => {
                                while let Ok(row) = exec.next() {
                                    let actual_name = unsafe { core::str::from_utf8_unchecked(row[0].to_chars().unwrap()) };
                                    let name = unsafe { core::str::from_utf8_unchecked(row[1].to_chars().unwrap()) };
                                    if let Err(e) = write!(
                                        self.chunk_writer,
                                        "<div><span class=\"size\">{} B</span><a>{};{}</a></div><br>",
                                        row[2].to_int().unwrap(),
                                        actual_name,
                                        name
                                    ).await {
                                        return Ok(Err(e));
                                    }
                                }
                            },
                            Err(_) => {
                                if let Err(e) = self.chunk_writer.write_chunk(b"<i>table empty</i><br>").await {
                                    return Ok(Err(e));
                                }
                            }
                        };
                    }

                    if let Err(e) = self.chunk_writer.write_chunk(MUSIC_LIST_PAGE.as_bytes()).await {
                        return Ok(Err(e));
                    }
                },
                Err(e) => {
                    if let Err(e) = self.chunk_writer.write_chunk(format!("error: {:?}", e).as_bytes()).await {
                        return Ok(Err(e));
                    }
                }
            }
            return Ok(self.chunk_writer.finalize().await);
        }
    }
}

pub struct MusicIterChunks {
    #[cfg(feature = "embassy-mode")]
    pub fman: &'static FMan,
    #[cfg(feature = "std-mode")]
    pub fman: &'static FMan,
}

impl Chunks for MusicIterChunks {
    fn content_type(&self) -> &'static str {
        "text/html"
    }

    async fn write_chunks<W: picoserve::io::Write>(
        self,
        mut chunk_writer: ChunkWriter<W>,
    ) -> Result<ChunksWritten, W::Error> {
        if self.fman.is_card_active().await {
            match self.fman.with_root_dir_async(HandleMusicAsync { chunk_writer }).await {
                Ok(res) => res,
                Err(_) => unreachable!()
            }
        } else {
            chunk_writer.write_chunk(b"SD Card not active").await?;
            chunk_writer.finalize().await
        }
    }
}

pub async fn handle_fs(path: String) -> impl IntoResponse {
    #[cfg(feature = "embassy-mode")]
    let fman = get_file_manager().await;
    #[cfg(feature = "std-mode")]
    let fman = get_file_manager();

    let file = fman.resolve_path_iter(&path).await;

    #[cfg(feature = "std-mode")] {
        ChunkedResponse::new(fs::FsIterChunks::<BlkDev> { 
            file, fman
        })
    }

    #[cfg(feature = "embassy-mode")] {
        ChunkedResponse::new(fs::FsIterChunks::<BlkDev> { 
            file, fman
        })
    }
}

pub async fn handle_music_list() -> impl IntoResponse {
    #[cfg(feature = "embassy-mode")]
    let fman = get_file_manager().await;
    #[cfg(feature = "std-mode")]
    let fman = get_file_manager();

    ChunkedResponse::new(MusicIterChunks {
        fman
    })
}

pub async fn handle_music_info() -> impl IntoResponse {
    #[cfg(feature = "embassy-mode")]
    let fman = get_file_manager().await;
    #[cfg(feature = "std-mode")]
    let fman = get_file_manager();

    fman.with_root_dir_async(delete::DeleteDbAsync).await
        .map_err(|e| picoserve::response::DebugValue(e))
}

pub async fn handle_music_data() -> impl IntoResponse {
    #[cfg(feature = "embassy-mode")]
    let fman = get_file_manager().await;
    #[cfg(feature = "std-mode")]
    let fman = get_file_manager();

    fman.with_root_dir_async(delete::DeleteDbAsync)
        .await.map_err(|e| picoserve::response::DebugValue(e))
}
