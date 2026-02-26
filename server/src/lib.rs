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
pub mod statics;
pub mod fs;

use alpa::embedded_sdmmc_fs::{DbDirSdmmc, VM};
use alpa::db::Database;
use alpa::{Query, QueryExecutor, Value};
use embedded_sdmmc::{BlockDevice, RawDirectory, VolumeManager, RawFile, Mode};
use picoserve::routing::{PathDescription};
use picoserve::response::status::StatusCode;
use picoserve::response::{IntoResponse, DebugValue};
use picoserve::request::{RequestBody, RequestParts, Path};
use picoserve::extract::{FromRequest};
use picoserve::io::{Read, Write};
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
pub static LIST_PAGE: &str = include_str!("./html/list.html");

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

struct HandleListAsync<W: Write> {
    chunk_writer: ChunkWriter<W>,
    dir_name: String
}

impl<W> AsyncRootFn<Result<ChunksWritten, W::Error>> for HandleListAsync<W>
where W: Write,
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
                

                    let files_table = match db.get_table(self.dir_name.as_str(), ExtAlloc::default()) {
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

                    if let Err(e) = self.chunk_writer.write_chunk(LIST_PAGE.as_bytes()).await {
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

pub struct ListState {
    fman: &'static FMan,
    dir_name: String
}

impl Chunks for ListState {
    fn content_type(&self) -> &'static str {
        "text/html"
    }

    async fn write_chunks<W: Write>(
        self,
        mut chunk_writer: ChunkWriter<W>,
    ) -> Result<ChunksWritten, W::Error> {
        if self.dir_name != consts::MUSIC_DIR && self.dir_name != consts::FILES_DIR {
            chunk_writer.write_chunk(b"invalid dir name").await?;
            return chunk_writer.finalize().await;
        }

        if self.fman.is_card_active().await {
            match self.fman.with_root_dir_async(HandleListAsync { chunk_writer, dir_name: self.dir_name }).await {
                Ok(res) => res,
                Err(_) => unreachable!()
            }
        } else {
            chunk_writer.write_chunk(b"SD Card not active").await?;
            chunk_writer.finalize().await
        }
    }
}

pub async fn handle_list(dir_name: String) -> impl IntoResponse {
    #[cfg(feature = "embassy-mode")]
    let fman = get_file_manager().await;
    #[cfg(feature = "std-mode")]
    let fman = get_file_manager();

    ChunkedResponse::new(ListState {
        fman, dir_name
    })
}

struct HandleMusicGetChunkAsync<W: Write> {
    chunk_writer: ChunkWriter<W>,
    raw_file: RawFile,
    idx: usize
}

impl<W> AsyncRootFn<Result<ChunksWritten, W::Error>> for HandleMusicGetChunkAsync<W>
where W: Write,
{
    type Fut<'a> = impl core::future::Future<
        Output = Result<Result<ChunksWritten, W::Error>, FManError<<FsBlockDevice as BlockDevice>::Error>>>
        + 'a where Self: 'a;

    fn call<'a>(mut self, root_dir: RawDirectory, vm: &'a VolumeManager<BlkDev, DummyTimesource, 4, 4, 1>) -> Self::Fut<'a> {
        async move {
            vm.close_dir(root_dir)?;

            let mut buf = statics::get_buf().await;
            let file = self.raw_file.to_file(vm);

            let offset = self.idx * 16 * 1024;
            let file_len = file.length() as usize;

            if file_len <= offset {
                return Ok(self.chunk_writer.finalize().await);
            }

            file.seek_from_start(offset as u32)?;

            let total_to_send = 16 * 1024;
            let mut bytes_sent = 0;
            let remaining_in_file = file_len - offset;
            let limit = total_to_send.min(remaining_in_file);

            while bytes_sent < limit {
                let chunk_size = (limit - bytes_sent).min(buf.len());
                file.read(&mut buf[..chunk_size])?;
                self.chunk_writer.write_chunk(&buf[..chunk_size]).await.map_err(|_| "unable to write chunk")?;
                bytes_sent += chunk_size;
            }

            Ok(self.chunk_writer.finalize().await)
        }
    }
}

pub struct MusicGetChunkState {
    fman: &'static FMan,
    raw_file: RawFile,
    idx: usize
}

impl Chunks for MusicGetChunkState {
    fn content_type(&self) -> &'static str {
        "text/html"
    }

    async fn write_chunks<W: Write>(self, mut chunk_writer: ChunkWriter<W>) -> Result<ChunksWritten, W::Error> {
        if self.fman.is_card_active().await {
            match self.fman.with_root_dir_async(HandleMusicGetChunkAsync {
                chunk_writer: chunk_writer,
                raw_file: self.raw_file,
                idx: self.idx
            }).await {
                Ok(res) => res,
                Err(_) => unreachable!()
            }
        } else {
            chunk_writer.write_chunk(b"SD Card not active").await?;
            chunk_writer.finalize().await
        }
    }
}

#[derive(serde::Deserialize)]
pub struct MusicChunkQuery {
    idx: usize
}

pub async fn handle_get_chunk((dir_name, id): (String, String), query: picoserve::extract::Query<MusicChunkQuery>) -> impl IntoResponse {
    #[cfg(feature = "embassy-mode")]
    let fman = get_file_manager().await;
    #[cfg(feature = "std-mode")]
    let fman = get_file_manager();

    fman.with_root_dir(move |root_dir, vm| {
        let root_dir = root_dir.to_directory(vm);

        if dir_name != consts::MUSIC_DIR && dir_name != consts::FILES_DIR {
            return Err("invalid dir name".into());
        }

        let music_dir = root_dir.open_dir(dir_name.as_str())?;
        let raw_file = music_dir.open_file_in_dir(id.as_str(), Mode::ReadOnly)?.to_raw_file();
        
        music_dir.close()?;
        root_dir.close()?;

        Ok(ChunkedResponse::new(MusicGetChunkState {
            fman, raw_file, idx: query.idx
        }))
    }).await.map_err(|e| {
        picoserve::response::Response::new(StatusCode::INTERNAL_SERVER_ERROR, format!("{:?}", e))
    })
}
