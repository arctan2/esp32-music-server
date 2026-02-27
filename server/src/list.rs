use super::*;

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

                        if let Err(e) = self.chunk_writer.write_chunk(b"<div id=\"list\">").await {
                            return Ok(Err(e));
                        }

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

                        if let Err(e) = self.chunk_writer.write_chunk(b"</div>").await {
                            return Ok(Err(e));
                        }
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
    .into_response()
    .with_header("Access-Control-Allow-Origin", "*")
    .with_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
    .with_header("Access-Control-Allow-Headers", "*")
}

