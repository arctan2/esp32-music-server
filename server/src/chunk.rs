use embedded_sdmmc::{Mode};
use alloc::format;
use super::*;
use picoserve::response::{Response, DebugValue};

#[allow(unused)]
#[cfg(feature = "std-mode")]
use std::println;
#[cfg(feature = "embassy-mode")]
use esp_println::println;

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
                    Err(_) => {
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

struct HandleGetChunkAsync<W: Write> {
    chunk_writer: ChunkWriter<W>,
    raw_file: RawFile,
    idx: usize
}

impl<W> AsyncRootFn<Result<ChunksWritten, W::Error>> for HandleGetChunkAsync<W>
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

pub struct GetChunkState {
    fman: &'static FMan,
    raw_file: RawFile,
    idx: usize
}

impl Chunks for GetChunkState {
    fn content_type(&self) -> &'static str {
        "text/html"
    }

    async fn write_chunks<W: Write>(self, mut chunk_writer: ChunkWriter<W>) -> Result<ChunksWritten, W::Error> {
        if self.fman.is_card_active().await {
            match self.fman.with_root_dir_async(HandleGetChunkAsync {
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
pub struct GetChunkQuery {
    idx: usize
}

pub async fn handle_get_chunk((dir_name, id): (String, String), query: picoserve::extract::Query<GetChunkQuery>) -> impl IntoResponse {
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

        Ok(ChunkedResponse::new(GetChunkState {
            fman, raw_file, idx: query.idx
        }))
    }).await.map_err(|e| {
        Response::new(StatusCode::INTERNAL_SERVER_ERROR, format!("{:?}", e))
    })
}

