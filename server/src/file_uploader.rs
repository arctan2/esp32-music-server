#![allow(unused)]
use alpa::embedded_sdmmc_fs::{DbDirSdmmc, VM};
use alpa::db::Database;
use alpa::{Value, Row, Query, QueryExecutor};
use embedded_sdmmc::{Mode, RawDirectory, VolumeManager, BlockDevice};
use picoserve::request::{RequestBody, RequestParts};
use picoserve::io::Read;
use file_manager::{get_file_manager, ExtAlloc, AsyncRootFn, FManError, DummyTimesource, BlkDev, FsBlockDevice};
use crate::consts;
use alloc::format;
use crate::chunks;
use allocator_api2::boxed::Box;
use allocator_api2::vec::Vec;
use chunks::select;
#[cfg(feature = "std-mode")]
use std::println;
#[cfg(feature = "embassy-mode")]
use esp_println::println;

#[cfg(feature = "embassy-mode")]
use embassy_futures::select::{Either};

pub async fn upload_file_to_dir<'r, R: Read>(
    parts: RequestParts<'r>,
    body: RequestBody<'r, R>,
    file_dir_name: &'static str,
    table_and_count_tracker_name: &'static str
) -> Result<(), &'static str> {
    let query_params = parts.query().ok_or("missing extension query")?;
    if query_params.is_empty() {
        return Err("missing extension query".into());
    }

    let mut s = query_params.0.split('=');
    let _ = s.next().unwrap();
    let ext = s.next().unwrap();

    let file_ext = format!("{}", ext);

    let mut reader = body.reader();

    let boundary_key = "boundary=";
    let content_type = parts.headers().get("Content-Type").ok_or("Content-Type not found")?;
    let boundary_start = content_type.as_str().unwrap().find(boundary_key).ok_or("boundary not found")?;
    let boundary = &content_type.as_raw()[boundary_start + boundary_key.len()..];
    let mut boundary_vec = Vec::with_capacity_in(boundary.len(), ExtAlloc::default());
    boundary_vec.extend_from_slice(boundary);
    let boundary = boundary_vec;

    let lookback_buf = Box::new_in([0u8; 128], ExtAlloc::default());
    let filename = Box::new_in([0u8; 128], ExtAlloc::default());

    chunks::send_event_sig(
        chunks::UploadEvent::NewReq{file_dir_name, filename, lookback_buf, table_and_count_tracker_name, boundary, file_ext}
    ).await;

    chunks::wait_ret_sig().await?;

    let ready_chan = chunks::get_ready_chan().await;
    let free_chan = chunks::get_free_chan().await;
    let mut size = 0;

    loop {
        let fut1 = chunks::wait_ret_sig();
        let fut2 = free_chan.recv();
        #[cfg(feature = "std-mode")] {
            select!(
                res = fut1 => {
                    return match res {
                        Ok(_) => Ok(()),
                        Err(e) => return Err(e.into())
                    };
                },
                mut chunk = fut2 => {
                    match reader.read(&mut chunk.buf).await {
                        Ok(n) => {
                            if n == 0 {
                                chunk.reset();
                                free_chan.send(chunk).await;
                                chunks::send_event_sig(chunks::UploadEvent::EndOfUpload).await;
                                break;
                            }
                            chunk.len = n;
                            ready_chan.send(chunk).await;
                        }
                        Err(e) => {
                            println!("error in second = {:?}", e);
                            chunk.reset();
                            free_chan.send(chunk).await;
                            chunks::send_event_sig(chunks::UploadEvent::ReadErr).await;
                            break;
                        }
                    }
                }
            );
        } #[cfg(feature = "embassy-mode")] {
            match select(fut1, fut2).await {
                Either::First(res) => {
                    return match res {
                        Ok(_) => Ok(()),
                        Err(e) => Err(e.into())
                    };
                }
                Either::Second(mut chunk) => {
                    match reader.read(&mut chunk.buf).await {
                        Ok(n) => {
                            if n == 0 {
                                chunk.reset();
                                free_chan.send(chunk).await;
                                chunks::send_event_sig(chunks::UploadEvent::EndOfUpload).await;
                                break;
                            }
                            chunk.len = n;
                            ready_chan.send(chunk).await;
                        }
                        Err(e) => {
                            println!("error in second = {:?}", e);
                            chunk.reset();
                            free_chan.send(chunk).await;
                            chunks::send_event_sig(chunks::UploadEvent::ReadErr).await;
                            break;
                        }
                    }
                }
            }
        }
    }

    chunks::get_ret_sig().await.wait().await
}

/*

#REQ
DELETE http://192.168.0.103/fs-delete/1.MP3

#ARGS

#RES

#END
*/
