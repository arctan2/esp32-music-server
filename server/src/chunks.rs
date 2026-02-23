#![allow(unused)]
use allocator_api2::boxed::Box;
use allocator_api2::vec::Vec;
use file_manager::runtime::{Channel, Signal, Mutex};
use alpa::embedded_sdmmc_fs::{DbDirSdmmc, VM};
use alpa::db::Database;
use alpa::{Value, Row, Query, QueryExecutor};
use file_manager::{
    get_file_manager,
    ExtAlloc,
    AsyncRootFn,
    FManError,
    DummyTimesource,
    BlkDev,
    FsBlockDevice
};
use crate::consts;
use embedded_sdmmc::{RawFile, VolumeManager, BlockDevice, TimeSource, RawDirectory, Directory, Mode, File};
use crate::String;
use crate::format;

#[cfg(feature = "std-mode")]
pub use crate::tokio_impl::*;

#[cfg(feature = "embassy-mode")]
pub use crate::embassy_impl::*;

pub const CHUNK_SIZE: usize = 1024;

#[derive(Debug)]
pub struct Chunk {
    pub len: usize,
    pub buf: [u8; CHUNK_SIZE]
}

impl Chunk {
    pub fn reset(&mut self) {
        self.buf.fill(0);
        self.len = 0;
    }
}

pub const CHAN_CAP: usize = 8;

#[derive(Debug)]
pub enum UploadEvent {
    NewReq {
        file_dir_name: &'static str,
        table_and_count_tracker_name: &'static str,
        filename: Box<[u8; 128], ExtAlloc>,
        lookback_buf: Box<[u8; 128], ExtAlloc>,
        boundary: Vec<u8, ExtAlloc>,
        file_ext: String
    },
    EndOfUpload,
    ReadErr
}

#[derive(Debug)]
pub enum RetSig {
    NewReq(Result<(), &'static str>),
    EndReq(Result<(), &'static str>)
}

#[derive(Debug)]
pub struct UploadRetVal {
    pub filename_len: usize,
    pub file_size: i64
}

pub async fn send_event_sig(msg: UploadEvent) {
    let sig = get_event_sig().await;
    sig.reset();
    sig.signal(msg).await;
}

pub async fn send_ret_sig(msg: Result<(), &'static str>) {
    let sig = get_ret_sig().await;
    sig.reset();
    sig.signal(msg).await;
}

enum Step {
    FindFilename,
    ReadFilename,
    FindDataStart,
    StreamingBody,
}

fn find_boundary_across_buffers(buf1: &[u8], buf2: &[u8], pat: &[u8]) -> Option<(usize, usize)> {
    if let Some(pos) = buf1.windows(pat.len()).position(|w| w == pat) {
        return Some((1, pos));
    }

    if let Some(pos) = buf2.windows(pat.len()).position(|w| w == pat) {
        return Some((2, pos));
    }

    let len1 = buf1.len();
    let pat_len = pat.len();
    
    for i in 1..pat_len {
        let suffix_len = pat_len - i;
        
        if len1 >= i && buf2.len() >= suffix_len {
            let part1 = &buf1[len1 - i..];
            let part2 = &buf2[..suffix_len];
            
            if part1 == &pat[..i] && part2 == &pat[i..] {
                return Some((1, len1 - i));
            }
        }
    }

    None
}

pub async fn init() {
    init_signals();

    let mut free_chan: Channel<Box<Chunk, ExtAlloc>, CHAN_CAP> = Channel::new();
    let mut ready_chan: Channel<Box<Chunk, ExtAlloc>, CHAN_CAP> = Channel::new();

    init_chunks(free_chan, ready_chan);

    let free_chan = get_free_chan().await;

    for i in 0..CHAN_CAP {
        let chunk = Box::new_in(Chunk{ len: 0, buf: [0; CHUNK_SIZE] }, ExtAlloc::default());
        free_chan.send(chunk).await;
    }
}

pub async fn task_file_uploader() {
    let free_chan = get_free_chan().await;
    let ready_chan = get_ready_chan().await;

    loop {
        println!("------------------------------waiting for event--------------------------------------------------");
        let sig = get_event_sig().await;
        let event = sig.wait().await;
        println!("received new event!: {:?}", event);
        match event {
            UploadEvent::NewReq{boundary, filename, lookback_buf, file_ext, table_and_count_tracker_name, file_dir_name} => {
                send_ret_sig(
                    handle_new_req(file_dir_name, filename, lookback_buf, table_and_count_tracker_name, boundary, file_ext).await
                ).await;
            }
            _ => ()
        }
    }
}

async fn handle_new_req(
    file_dir_name: &'static str,
    filename: Box<[u8; 128], ExtAlloc>,
    lookback_buf: Box<[u8; 128], ExtAlloc>,
    table_and_count_tracker_name: &'static str,
    boundary: Vec<u8, ExtAlloc>,
    file_ext: String,
) -> Result<(), &'static str> {
    #[cfg(feature = "embassy-mode")]
    let fman = get_file_manager().await;
    #[cfg(feature = "std-mode")]
    let fman = get_file_manager();

    let uploader_async = FileUploaderAsync {
        file_dir_name, filename, lookback_buf, table_and_count_tracker_name, boundary, file_ext
    };

    fman.with_root_dir_async(uploader_async).await.map_err(|e| {
        "error while upload_file_to_dir"
    })
}

struct FileUploaderAsync {
    file_dir_name: &'static str,
    table_and_count_tracker_name: &'static str,
    filename: Box<[u8; 128], ExtAlloc>,
    lookback_buf: Box<[u8; 128], ExtAlloc>,
    boundary: Vec<u8, ExtAlloc>,
    file_ext: String,
}

impl AsyncRootFn<()> for FileUploaderAsync {
    type Fut<'a> = impl core::future::Future<Output = Result<(), FManError<<FsBlockDevice as BlockDevice>::Error>>> + 'a where Self: 'a;

    fn call<'a>(self, root_dir: RawDirectory, vm: &'a VolumeManager<BlkDev, DummyTimesource, 4, 4, 1>) -> Self::Fut<'a> {
        async move {
            let root_dir = root_dir.to_directory(vm);
            let db_dir = root_dir.open_dir(consts::DB_DIR).map_err(|_| "unable to open db dir")?.to_raw_directory();
            let db_dir = DbDirSdmmc::new(db_dir);
            let mut db = match Database::new_init(VM::new(vm), db_dir, ExtAlloc::default()) {
                Ok(d) => d,
                Err(_) => return Err("db init error".into())
            };
            let count_tracker_table = db.get_table(consts::COUNT_TRACKER_TABLE, ExtAlloc::default())
                                        .map_err(|_| "unable to get count_tracker table")?;
            let files_table = db.get_table(self.table_and_count_tracker_name, ExtAlloc::default())
                                .map_err(|_| "unable to get files table")?;
            let cur_file_id: i64;

            {
                let query = Query::<_, &str>::new(count_tracker_table, ExtAlloc::default())
                                             .key(Value::Chars(self.table_and_count_tracker_name.as_bytes()));
                match QueryExecutor::new(
                    query, &mut db.table_buf, &mut db.buf1, &mut db.buf2,
                    &db.file_handler.page_rw.as_ref().unwrap()
                ) {
                    Ok(mut exec) => {
                        if let Ok(row) = exec.next() {
                            cur_file_id = row[1].to_int().unwrap();
                        } else {
                            return Err("bad init".into());
                        }
                    },
                    Err(_) => {
                        return Err("table empty".into());
                    }
                };
            }

            if cur_file_id < 0 || cur_file_id >= 99999999 {
                return Err("id limit reached".into());
            }

            let files_dir = root_dir.open_dir(self.file_dir_name).map_err(|_| "unable to open FILES dir")?;

            let ready_chan = get_ready_chan().await;
            let free_chan = get_free_chan().await;

            send_ret_sig(Ok(())).await;

            let mut filename = self.filename;
            let actual_name = format!("{}.{}", cur_file_id, self.file_ext);
            let file_info = fetch_parse_chunks(
                &ready_chan,
                files_dir,
                &mut filename,
                self.lookback_buf,
                &actual_name,
                vm,
                self.boundary
            ).await?;

            {
                let mut row = Row::new_in(ExtAlloc::default());
                row.push(Value::Chars(actual_name.as_bytes()));
                row.push(Value::Chars(&filename[..file_info.filename_len]));
                row.push(Value::Int(file_info.file_size));
                db.insert_to_table(files_table, row, ExtAlloc::default()).map_err(|_| "unable to insert to table")?;
            }

            {
                let mut row = Row::new_in(ExtAlloc::default());
                row.push(Value::Chars(self.table_and_count_tracker_name.as_bytes()));
                row.push(Value::Int(cur_file_id + 1));
                db.update_row(count_tracker_table, Value::Chars(self.table_and_count_tracker_name.as_bytes()), row, ExtAlloc::default())
                    .map_err(|_| "unable to update count_tracker_table to table")?;
            }

            Ok(())
        }
    }
}

async fn fetch_parse_chunks<'a>(
    ready_chan: &Channel<Box<Chunk, ExtAlloc>, CHAN_CAP>,
    files_dir: Directory<'a, BlkDev, DummyTimesource, 4, 4, 1>,
    filename: &mut Box<[u8; 128], ExtAlloc>,
    mut lookback_buf: Box<[u8; 128], ExtAlloc>,
    actual_name: &String,
    vm: &'a VolumeManager<BlkDev, DummyTimesource, 4, 4, 1>,
    boundary: Vec<u8, ExtAlloc>,
) -> Result<UploadRetVal, &'static str> {
    let new_file = files_dir.open_file_in_dir(actual_name.as_str(), Mode::ReadWriteCreate).map_err(|e| {
        println!("e = {:?}", e);
        "unable to create file"
    })?;
    let mut filename_len = 0;

    let mut step = Step::FindFilename;
    let mut pattern_idx = 0;

    let mut lookback_len = 0;
    let mut file_size: i64 = 0;
    
    let mut is_end_of_upload = false;
    let free_chan = get_free_chan().await;
    let mut is_boundary_found = false;

    loop {
        if is_end_of_upload {
            while let Some(mut chunk) = ready_chan.try_recv().await {
                match handle_chunk(
                    chunk.as_ref(),
                    &mut step,
                    filename,
                    &mut filename_len,
                    &mut lookback_buf,
                    &mut lookback_len,
                    &mut pattern_idx,
                    &new_file,
                    &mut file_size,
                    &boundary
                ) {
                    Ok(v) => {
                        chunk.reset();
                        free_chan.send(chunk).await;
                        is_boundary_found |= v;
                    },
                    Err(e) => {
                        chunk.reset();
                        free_chan.send(chunk).await;
                        new_file.close().map_err(|_| "unable to close file")?;
                        files_dir.delete_file_in_dir(actual_name.as_str()).map_err(|_| "unable to delete file")?;
                        return Err(e);
                    }
                }
            }

            if !is_boundary_found {
                return Err("boundary not found");
            }

            new_file.flush().map_err(|_| "unable to flush file")?;
            new_file.close().map_err(|_| "unable to close file")?;

            return Ok(UploadRetVal{
                filename_len,
                file_size
            });
        }

        let fut1 = get_event_sig().await.wait();
        let fut2 = ready_chan.recv();

        #[cfg(feature = "std-mode")] {
            select!(
                event = fut1 => {
                    match event {
                        UploadEvent::ReadErr => {
                            new_file.close().map_err(|_| "unable to close file")?;
                            files_dir.delete_file_in_dir(actual_name.as_str()).map_err(|_| "unable to delete file")?;
                            return Err("read error");
                        }
                        UploadEvent::EndOfUpload => {
                            is_end_of_upload = true;
                        }
                        _ => ()
                    }
                },
                mut chunk = fut2 => {
                    match handle_chunk(
                        chunk.as_ref(),
                        &mut step,
                        filename,
                        &mut filename_len,
                        &mut lookback_buf,
                        &mut lookback_len,
                        &mut pattern_idx,
                        &new_file,
                        &mut file_size,
                        &boundary
                    ) {
                        Ok(v) => {
                            chunk.reset();
                            free_chan.send(chunk).await;
                            is_boundary_found |= v;
                        },
                        Err(e) => {
                            chunk.reset();
                            free_chan.send(chunk).await;
                            new_file.close().map_err(|_| "unable to close file")?;
                            files_dir.delete_file_in_dir(actual_name.as_str()).map_err(|_| "unable to delete file")?;
                            return Err(e);
                        }
                    }
                }
            );
        } #[cfg(feature = "embassy-mode")] {
            match select(fut1, fut2).await {
                Either::First(event) => {
                    match event {
                        UploadEvent::ReadErr => {
                            new_file.close().map_err(|_| "unable to close file")?;
                            files_dir.delete_file_in_dir(actual_name.as_str()).map_err(|_| "unable to delete file")?;
                            return Err("read error");
                        }
                        UploadEvent::EndOfUpload => {
                            is_end_of_upload = true;
                        }
                        _ => ()
                    }
                }
                Either::Second(mut chunk) => {
                    match handle_chunk(
                        chunk.as_ref(),
                        &mut step,
                        filename,
                        &mut filename_len,
                        &mut lookback_buf,
                        &mut lookback_len,
                        &mut pattern_idx,
                        &new_file,
                        &mut file_size,
                        &boundary
                    ) {
                        Ok(v) => {
                            chunk.reset();
                            free_chan.send(chunk).await;
                            is_boundary_found |= v;
                        },
                        Err(e) => {
                            chunk.reset();
                            free_chan.send(chunk).await;
                            new_file.close().map_err(|_| "unable to close file")?;
                            files_dir.delete_file_in_dir(actual_name.as_str()).map_err(|_| "unable to delete file")?;
                            return Err(e);
                        }
                    }
                }
            }
        }
    }
}

fn handle_chunk<'a>(
    chunk: &Chunk,
    step: &mut Step,
    filename: &mut Box<[u8; 128], ExtAlloc>,
    filename_len: &mut usize,
    lookback_buf: &mut Box<[u8; 128], ExtAlloc>,
    lookback_len: &mut usize,
    pattern_idx: &mut usize,
    new_file: &File<'a, BlkDev, DummyTimesource, 4, 4, 1>,
    file_size: &mut i64,
    boundary: &Vec<u8, ExtAlloc>
) -> Result<bool, &'static str> {
    if chunk.len == 0 {
        return Ok(false);
    }

    for (i, &byte) in chunk.buf[..chunk.len].iter().enumerate() {
        match step {
            Step::FindFilename => {
                let file_pat = b"filename=\"";
                if byte == file_pat[*pattern_idx] {
                    *pattern_idx += 1;
                    if *pattern_idx == file_pat.len() {
                        *step = Step::ReadFilename;
                        *pattern_idx = 0;
                    }
                } else {
                    *pattern_idx = 0;
                }
            }

            Step::ReadFilename => {
                if byte == '"' as u8 {
                    *step = Step::FindDataStart;
                    *pattern_idx = 0;
                } else {
                    filename[*filename_len] = byte;
                    *filename_len += 1;
                }
            }

            Step::FindDataStart => {
                let header_sep = b"\r\n\r\n";
                if byte == header_sep[*pattern_idx] {
                    *pattern_idx += 1;
                    if *pattern_idx == header_sep.len() {
                        *step = Step::StreamingBody;
                    }
                } else {
                    *pattern_idx = 0;
                }
            }

            Step::StreamingBody => {
                let data_chunk = &chunk.buf[i..chunk.len];

                if let Some(pos) = find_boundary_across_buffers(&lookback_buf[..*lookback_len], data_chunk, boundary) {
                    match pos {
                        (1, idx) => {
                            let end = idx.saturating_sub(4);
                            let buf = &lookback_buf[0..end];
                            new_file.write(buf).map_err(|_| "unable to write to new_file (1, idx)")?;
                            *file_size += buf.len() as i64;
                        },
                        (2, idx) => {
                            if idx >= 4 {
                                let buf = &lookback_buf[..*lookback_len];
                                new_file.write(buf).map_err(|_| "unable to write to new_file (2, idx) 1")?;
                                *file_size += buf.len() as i64;

                                let buf = &data_chunk[0..idx-4];
                                new_file.write(buf).map_err(|_| "unable to write to new_file (2, idx) 2")?;
                                *file_size += buf.len() as i64;
                            } else {
                                let lookback_trim = 4 - idx;
                                let end = (*lookback_len).saturating_sub(lookback_trim);
                                let buf = &lookback_buf[..end];
                                new_file.write(buf).map_err(|_| "unable to write to new_file (2, idx) else")?;
                                *file_size += buf.len() as i64;
                            }
                        },
                        _ => unreachable!()
                    }
                    return Ok(true);
                } else {
                    let buf = &lookback_buf[..*lookback_len];
                    new_file.write(buf).map_err(|_| "unable to write to new_file else 1")?;
                    *file_size += buf.len() as i64;

                    if data_chunk.len() >= lookback_buf.len() {
                        let safe_len = data_chunk.len() - lookback_buf.len();
                        let buf = &data_chunk[..safe_len];

                        new_file.write(buf).map_err(|_| "unable to write to new_file else if 1")?;
                        *file_size += buf.len() as i64;

                        let tail = &data_chunk[safe_len..];
                        lookback_buf[..tail.len()].copy_from_slice(tail);
                        *lookback_len = tail.len();
                    } else {
                        let move_amt = data_chunk.len();
                        lookback_buf.copy_within(move_amt.., 0);

                        let start = lookback_buf.len() - move_amt;
                        lookback_buf[start..].copy_from_slice(data_chunk);
                        *lookback_len = lookback_buf.len();
                    }
                }
                break;
            }
        }
    }
    Ok(false)
}

