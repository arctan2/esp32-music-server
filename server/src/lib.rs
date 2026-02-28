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

pub mod chunk;
pub mod list;
pub mod delete;
pub mod upload;
pub mod statics;
pub mod fs;
pub mod range;

use alpa::embedded_sdmmc_fs::{DbDirSdmmc, VM};
use alpa::db::Database;
use alpa::{Query, QueryExecutor, Value};
use embedded_sdmmc::{BlockDevice, RawDirectory, VolumeManager, RawFile};
use picoserve::routing::{PathDescription};
use picoserve::response::status::StatusCode;
use picoserve::response::{IntoResponse};
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

pub static HOME_PAGE: &str = include_str!("./htm/home.htm");
pub static LIST_PAGE: &str = include_str!("./htm/list.htm");

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
