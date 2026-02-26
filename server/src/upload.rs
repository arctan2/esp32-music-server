use super::*;
use picoserve::response::{IntoResponse};
use file_manager::{IntAlloc};
use alpa::{db::Database, Row};
use embedded_sdmmc::Mode;

#[cfg(feature = "std-mode")]
use std::println;

#[cfg(feature = "embassy-mode")]
use esp_println::println;

#[derive(serde::Deserialize)]
pub struct NewQuery {
    name: String,
    ext: String,
    size: usize
}

#[derive(serde::Deserialize)]
pub struct EndQuery {
    id: String,
}

pub async fn new(query: picoserve::extract::Query<NewQuery>) -> impl IntoResponse {
    #[cfg(feature = "embassy-mode")]
    let fman = get_file_manager().await;
    #[cfg(feature = "std-mode")]
    let fman = get_file_manager();

    fman.with_root_dir(|root_dir, vm| {
        let root_dir = root_dir.to_directory(vm);
        let db_dir = root_dir.open_dir(consts::DB_DIR).map_err(|_| "unable to open db dir")?.to_raw_directory();
        let db_dir = DbDirSdmmc::new(db_dir);
        let mut db = match Database::new_init(VM::new(vm), db_dir, IntAlloc::default()) {
            Ok(d) => d,
            Err(_) => return Err("db init error".into())
        };
        let count_tracker_table = db.get_table(consts::COUNT_TRACKER_TABLE, IntAlloc::default())
                                    .map_err(|_| "unable to get count_tracker table")?;
        let music_table = db.get_table(consts::MUSIC_TABLE, IntAlloc::default())
                            .map_err(|_| "unable to get music table")?;
        let cur_file_id: i64;

        {
            let query = Query::<_, &str>::new(count_tracker_table, IntAlloc::default())
                                         .key(Value::Chars(consts::MUSIC_TABLE.as_bytes()));
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

        let music_dir = root_dir.open_dir(consts::MUSIC_DIR).map_err(|_| "unable to open dir")?;
        let actual_name = format!("{}.{}", cur_file_id, query.ext);
        let file = music_dir.open_file_in_dir(actual_name.as_str(), Mode::ReadWriteCreate)
                            .map_err(|e| {
                                println!("e = {:?}", e);
                                "unable to create new file"
                            })?;

        file.close().map_err(|_| "unable to close new file")?;

        {
            let mut row = Row::new_in(IntAlloc::default());
            row.push(Value::Chars(actual_name.as_bytes()));
            row.push(Value::Chars(&query.name.as_bytes()));
            row.push(Value::Int(query.size as i64));
            db.insert_to_table(music_table, row, IntAlloc::default()).map_err(|e| {
                println!("e = {:?}", e);
                "unable to insert to table"
            })?;
        }

        {
            let mut row = Row::new_in(IntAlloc::default());
            row.push(Value::Chars(consts::MUSIC_TABLE.as_bytes()));
            row.push(Value::Int(cur_file_id + 1));
            db.update_row(count_tracker_table, Value::Chars(consts::MUSIC_TABLE.as_bytes()), row, IntAlloc::default())
                .map_err(|_| "unable to update count_tracker_table to table")?;
        }

        Ok(picoserve::response::DebugValue(format!("success: {}", actual_name)))
    }).await
    .map_err(|e| {
        picoserve::response::DebugValue(e)
    })
}

pub struct MusicReceiver {
    response: String
}

impl<'r, State> FromRequest<'r, State> for MusicReceiver {
    type Rejection = picoserve::response::DebugValue<String>;

    async fn from_request<R: Read>(
        _state: &'r State,
        parts: RequestParts<'r>,
        body: RequestBody<'r, R>,
    ) -> Result<Self, Self::Rejection> {
        chunk_receiver::receive_chunks(parts, body, consts::MUSIC_DIR).await.map(|response| Self { response })
    }
}

pub async fn chunk(data: MusicReceiver) -> impl IntoResponse {
    picoserve::response::DebugValue(data.response)
}

pub async fn end(query: picoserve::extract::Query<EndQuery>) -> impl IntoResponse {
    #[cfg(feature = "embassy-mode")]
    let fman = get_file_manager().await;
    #[cfg(feature = "std-mode")]
    let fman = get_file_manager();

    fman.with_root_dir(|root_dir, vm| {
        let root_dir = root_dir.to_directory(vm);
        let music_dir = root_dir.open_dir(consts::MUSIC_DIR).map_err(|_| "unable to open dir")?;
        let _ = music_dir.open_file_in_dir(query.id.as_str(), Mode::ReadWriteAppend).map_err(|_| "file not found")?;
        Ok("success")
    }).await
    .map_err(|e| picoserve::response::DebugValue(e))
}
