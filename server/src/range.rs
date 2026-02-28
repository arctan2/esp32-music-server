use embedded_sdmmc::{Mode};
use alloc::format;
use super::*;

#[allow(unused)]
#[cfg(feature = "std-mode")]
use std::println;
#[cfg(feature = "embassy-mode")]
use esp_println::println;

struct HandleRangeAsync<W: Write> {
    chunk_writer: ChunkWriter<W>,
    raw_file: RawFile,
    start: usize,
    len: usize,
}

impl<W> AsyncRootFn<Result<ChunksWritten, W::Error>> for HandleRangeAsync<W>
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

            file.seek_from_start(self.start as u32)?;

            let mut bytes_sent = 0;

            while bytes_sent < self.len {
                let chunk_size = (self.len - bytes_sent).min(buf.len());
                file.read(&mut buf[..chunk_size])?;
                self.chunk_writer.write_chunk(&buf[..chunk_size]).await.map_err(|_| "unable to write chunk")?;
                bytes_sent += chunk_size;
            }

            Ok(self.chunk_writer.finalize().await)
        }
    }
}

pub struct RangeState {
    fman: &'static FMan,
    raw_file: RawFile,
    start: usize,
    len: usize,
}

impl Chunks for RangeState {
    fn content_type(&self) -> &'static str {
        "audio/mp3"
    }

    async fn write_chunks<W: Write>(self, mut chunk_writer: ChunkWriter<W>) -> Result<ChunksWritten, W::Error> {
        if self.fman.is_card_active().await {
            match self.fman.with_root_dir_async(HandleRangeAsync {
                chunk_writer: chunk_writer,
                raw_file: self.raw_file,
                start: self.start,
                len: self.len,
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

pub struct RangeExtractor {
    start: usize
}

impl <'r> picoserve::extract::FromRequestParts<'r, ()> for RangeExtractor {
    type Rejection = &'static str;

    async fn from_request_parts(
        _state: &'r (),
        parts: &RequestParts<'r>,
    ) -> Result<Self, Self::Rejection> {
        let range_header = parts
            .headers()
            .get("Range")
            .and_then(|v| core::str::from_utf8(v.as_raw()).ok());

        let start: usize = if let Some(range) = range_header {
            if let Some(bytes_part) = range.strip_prefix("bytes=") {
                bytes_part
                    .split('-')
                    .next()
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(0)
            } else {
                0
            }
        } else {
            0
        };

        Ok(Self {
            start
        })
    }
}

pub async fn handle_range_request((dir_name, id): (String, String), range: RangeExtractor) -> impl IntoResponse {
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

        let total_len = vm.file_length(raw_file)? as usize;

        if range.start >= total_len {
            return Err("range out of bounds".into());
        }

        let chunk_size = 16 * 1024;
        let end = (range.start + chunk_size - 1).min(total_len - 1);
        let content_len = end - range.start + 1;

        Ok(
            ChunkedResponse::new(RangeState {
                fman,
                raw_file,
                start: range.start,
                len: content_len,
            }).into_response()
            .with_status_code(StatusCode::PARTIAL_CONTENT)
            .with_header("Accept-Ranges", "bytes")
            .with_header("Content-Length", content_len.to_string())
            .with_header(
                "Content-Range",
                format!("bytes {}-{}/{}", range.start, end, total_len),
            )
        )
    }).await.map_err(|e| {
        picoserve::response::Response::new(StatusCode::INTERNAL_SERVER_ERROR, format!("{:?}", e))
    })
}



