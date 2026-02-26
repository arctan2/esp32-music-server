use embedded_sdmmc::{Mode, RawDirectory, VolumeManager, BlockDevice};
use picoserve::request::{RequestBody, RequestParts};
use picoserve::io::Read;
use file_manager::{get_file_manager, ExtAlloc, AsyncRootFn, FManError, DummyTimesource, BlkDev, FsBlockDevice};
use alloc::format;
use allocator_api2::boxed::Box;
#[cfg(feature = "std-mode")]
use std::println;
#[cfg(feature = "embassy-mode")]
use esp_println::println;
use super::*;
use picoserve::response::DebugValue;

#[derive(serde::Deserialize)]
pub struct ChunkQuery {
    id: String,
    idx: usize
}

pub struct HandleChunkAsync<'r, R: Read> {
    query: ChunkQuery,
    body: RequestBody<'r, R>,
    file_dir_name: &'r str,
}

impl<'r, R> AsyncRootFn<String> for HandleChunkAsync<'r, R>
where R: Read
{
    type Fut<'a> = impl core::future::Future<
        Output = Result<String, FManError<<FsBlockDevice as BlockDevice>::Error>>> + 'a where Self: 'a;

    fn call<'a>(self, root_dir: RawDirectory, vm: &'a VolumeManager<BlkDev, DummyTimesource, 4, 4, 1>) -> Self::Fut<'a> {
        async move {
            let mut buf = crate::statics::get_buf().await;
            let root_dir = root_dir.to_directory(vm);

            let music_dir = root_dir.open_dir(self.file_dir_name)?;
            let file = music_dir.open_file_in_dir(self.query.id.as_str(), Mode::ReadWriteAppend)?;

            let mut reader = self.body.reader();

            let mut read = 0;

            file.seek_from_start((self.query.idx * 16 * 1024) as u32)?;

            loop {
                match reader.read(buf.as_mut()).await {
                    Ok(n) => {
                        if n == 0 {
                            break;
                        }
                        file.write(&buf.as_ref()[..n])?;
                        #[cfg(feature = "embassy-mode")]
                        embassy_futures::yield_now().await;
                        #[cfg(feature = "embassy-mode")]
                        embassy_time::Timer::after_ticks(5).await;
                        read += n;
                    }
                    Err(e) => {
                        println!("e = {:?}", e);
                        return Err("error while reading body".into());
                    }
                }
            }

            file.flush()?;
            Ok(format!("{}", read))
        }
    }
}

pub async fn receive_chunks<'r, R: Read>(
    parts: RequestParts<'r>,
    body: RequestBody<'r, R>,
    file_dir_name: &'r str,
) -> Result<String, DebugValue<String>> {
    #[cfg(feature = "embassy-mode")]
    let fman = get_file_manager().await;
    #[cfg(feature = "std-mode")]
    let fman = get_file_manager();
    
    let query_params = parts.query().ok_or(DebugValue(String::from("Bad Query")))?;
    let query: ChunkQuery = picoserve::url_encoded::deserialize_form(query_params).map_err(|_| DebugValue(String::from("Bad Query")))?;

    let h = HandleChunkAsync {
        query, body, file_dir_name
    };
    fman.with_root_dir_async(h).await.map_err(|e| DebugValue(format!("{:?}", e)))
}

/*

#REQ
DELETE http://192.168.0.103/fs-music-delete/1.MP3

#ARGS

#RES

success

#END
*/
